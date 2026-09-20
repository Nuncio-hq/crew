-- Contact fallback decision seam (issue #412, successor contract of #355).
--
-- This migration turns the R4 storage foundation into a production seam:
--   * the kind-9 classify trigger now admits an explicit non-zero class only
--     inside a transaction that raised `buzz.contact_decision_v1`, i.e. the
--     reviewed decision path;
--   * kind-46044 decision proofs share the kind-9 identity/retention guard;
--   * `contact_claims` records the fenced claim lifecycle for each routed
--     decision (pending -> claimed -> started -> completed | cancelled,
--     with `unknown` for lease expiry fencing);
--   * deleting (soft) a routed original cancels its open claims in the same
--     statement, and completing a claim requires a matching kind-46043
--     receipt parented to the decision;
--   * deferred reciprocal guards on every events leaf prove at COMMIT time
--     that every routed original has its route + relay-signed proof + claim
--     + quota parity — the guards are the atomicity contract, the Rust
--     decision path just writes the consistent set.
-- `contact_routes` gains `canvas_id`: the winning canvas is pinned at
-- decision time, never re-read.
SET LOCAL lock_timeout = '5s';

-- ---------------------------------------------------------------------------
-- 1. Decision-path gate on classified originals.
--    NULL still classifies to suppressed (0). A non-zero class requires the
--    transaction-local flag set by `Db::decide_contact_route_tx` (and nothing
--    else). The R4 fixture drops this trigger and installs its own catalog, so
--    the reviewed-guard tests keep running unchanged.
CREATE OR REPLACE FUNCTION contact_classify_original_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.kind = 9 THEN
            IF NEW.contact_class IS NULL THEN
                NEW.contact_class := 0;
            ELSIF NEW.contact_class <> 0 THEN
                IF current_setting('buzz.contact_decision_v1', true)
                    IS DISTINCT FROM 'on' THEN
                    RAISE EXCEPTION 'contact routing classification requires the reviewed decision path'
                        USING ERRCODE = 'check_violation';
                END IF;
            END IF;
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.contact_class IS DISTINCT FROM NEW.contact_class THEN
        RAISE EXCEPTION 'contact classification is immutable'
            USING ERRCODE = 'check_violation';
    END IF;

    -- Check both sides so a non-kind-9 row cannot be rewritten into a signed
    -- original and a legacy kind-9 row cannot be rewritten out of one.
    IF OLD.kind = 9 OR NEW.kind = 9 THEN
        IF ROW(OLD.community_id, OLD.id, OLD.created_at, OLD.kind)
            IS DISTINCT FROM ROW(NEW.community_id, NEW.id, NEW.created_at, NEW.kind)
        THEN
            RAISE EXCEPTION 'kind-9 original identity is immutable, including legacy rows'
                USING ERRCODE = 'check_violation';
        END IF;
    END IF;
    RETURN NEW;
END
$$;

-- 2. Extend the identity/retention guard to kind-46044 proofs: fully immutable
--    after insert (no soft delete, no edit), hard delete forbidden except the
--    fenced whole-community executor — same exemption as kind 9.
CREATE OR REPLACE FUNCTION contact_guard_original_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    executor_community TEXT;
    executor_generation TEXT;
    lifecycle TEXT;
    expected_generation BIGINT;
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF OLD.contact_class IS DISTINCT FROM NEW.contact_class THEN
            RAISE EXCEPTION 'contact classification is immutable'
                USING ERRCODE = 'check_violation';
        END IF;
        IF (OLD.kind = 9 OR NEW.kind = 9)
           AND ROW(OLD.community_id, OLD.id, OLD.created_at, OLD.kind)
               IS DISTINCT FROM ROW(NEW.community_id, NEW.id, NEW.created_at, NEW.kind)
        THEN
            RAISE EXCEPTION 'kind-9 original identity is immutable, including legacy rows'
                USING ERRCODE = 'check_violation';
        END IF;
        IF (OLD.kind = 46044 OR NEW.kind = 46044) AND NEW IS DISTINCT FROM OLD THEN
            RAISE EXCEPTION 'contact decision proof data is immutable'
                USING ERRCODE = 'check_violation';
        END IF;
        RETURN NEW;
    END IF;

    IF OLD.kind <> 9 AND OLD.kind <> 46044 THEN
        RETURN OLD;
    END IF;

    executor_community := current_setting('buzz.deletion_executor_community', true);
    executor_generation := current_setting('buzz.deletion_fence_generation', true);
    SELECT deletion_state, deletion_fence_generation
      INTO lifecycle, expected_generation
      FROM communities
     WHERE id = OLD.community_id;

    IF executor_community = OLD.community_id::TEXT
       AND executor_generation ~ '^[0-9]+$'
       AND executor_generation::BIGINT = expected_generation
       AND lifecycle IN ('fenced', 'tombstone')
    THEN
        RETURN OLD;
    END IF;

    RAISE EXCEPTION 'ordinary hard deletion of kind-9 originals and contact proofs is forbidden'
        USING ERRCODE = 'check_violation';
END
$$;

-- 3. Winning canvas pinned on the route row. Nullable: rows written by the
--    R4 fixture catalog carry no canvas; the production decision path always
--    sets it (asserted when present).
ALTER TABLE contact_routes ADD COLUMN canvas_id BYTEA;
ALTER TABLE contact_routes ADD CONSTRAINT contact_route_canvas_shape
    CHECK (canvas_id IS NULL OR octet_length(canvas_id) = 32);

-- 4. Claim lifecycle, one row per routed decision.
CREATE TABLE contact_claims (
    community_id     UUID NOT NULL REFERENCES communities(id),
    decision_id      BYTEA NOT NULL,
    original_id      BYTEA NOT NULL,
    channel_id       UUID NOT NULL,
    contact_pubkey   BYTEA NOT NULL,
    state            TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'claimed', 'started', 'unknown', 'completed', 'cancelled')),
    generation       BIGINT NOT NULL DEFAULT 0 CHECK (generation >= 0),
    holder           BYTEA CHECK (holder IS NULL OR octet_length(holder) = 32),
    lease_expires_at TIMESTAMPTZ,
    receipt_id       BYTEA CHECK (receipt_id IS NULL OR octet_length(receipt_id) = 32),
    claimed_at       TIMESTAMPTZ,
    started_at       TIMESTAMPTZ,
    terminal_at      TIMESTAMPTZ,
    PRIMARY KEY (community_id, decision_id),
    FOREIGN KEY (community_id, decision_id)
        REFERENCES contact_routes (community_id, decision_id),
    CHECK (octet_length(decision_id) = 32),
    CHECK (octet_length(original_id) = 32),
    CHECK (octet_length(contact_pubkey) = 32)
);
CREATE INDEX contact_claims_original
    ON contact_claims (community_id, original_id);
CREATE INDEX contact_claims_open_contact
    ON contact_claims (community_id, contact_pubkey)
    WHERE state IN ('claimed', 'started');

-- 5. Claim state machine. Generations never regress; completed needs its
--    receipt id; terminal states stamp terminal_at. Terminal rows are frozen.
CREATE FUNCTION contact_claim_transition_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    valid BOOLEAN;
BEGIN
    IF NEW.generation < OLD.generation THEN
        RAISE EXCEPTION 'contact claim generation must not regress'
            USING ERRCODE = 'check_violation';
    END IF;
    valid := (OLD.state, NEW.state) IN (
        ('pending', 'claimed'), ('pending', 'cancelled'),
        ('claimed', 'claimed'), ('claimed', 'started'),
        ('claimed', 'cancelled'), ('claimed', 'unknown'),
        ('started', 'started'), ('started', 'completed'),
        ('started', 'cancelled'), ('started', 'unknown'),
        ('unknown', 'claimed'), ('unknown', 'cancelled'),
        ('unknown', 'unknown')
    );
    IF NOT valid THEN
        RAISE EXCEPTION 'invalid contact claim transition % -> %', OLD.state, NEW.state
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.state = 'completed' AND NEW.receipt_id IS NULL THEN
        RAISE EXCEPTION 'completed contact claim requires its receipt id'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.state IN ('completed', 'cancelled') THEN
        NEW.terminal_at := clock_timestamp();
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER contact_claim_transition_v1
    BEFORE UPDATE ON contact_claims
    FOR EACH ROW EXECUTE FUNCTION contact_claim_transition_v1();

CREATE FUNCTION contact_claim_no_delete_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF contact_purge_executor_exempt_v1(OLD.community_id) THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'contact claim rows are evidence and are never deleted'
        USING ERRCODE = 'check_violation';
END
$$;
CREATE TRIGGER contact_claim_no_delete_v1
    BEFORE DELETE ON contact_claims
    FOR EACH ROW EXECUTE FUNCTION contact_claim_no_delete_v1();

-- Claims arrive only with their route and must echo it exactly.
CREATE FUNCTION contact_check_claim_insert_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    r contact_routes%ROWTYPE;
BEGIN
    SELECT * INTO r FROM contact_routes
     WHERE community_id = NEW.community_id AND decision_id = NEW.decision_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'contact claim has no route' USING ERRCODE = 'check_violation';
    END IF;
    IF r.original_id IS DISTINCT FROM NEW.original_id
       OR r.channel_id IS DISTINCT FROM NEW.channel_id
       OR r.contact_pubkey IS DISTINCT FROM NEW.contact_pubkey THEN
        RAISE EXCEPTION 'contact claim does not match its route'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END
$$;
CREATE CONSTRAINT TRIGGER contact_check_claim_insert_v1
    AFTER INSERT ON contact_claims DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION contact_check_claim_insert_v1();

-- Completing a claim requires a committed-in-this-transaction kind-46043
-- receipt authored by the claim's contact and e-tagged to the decision.
CREATE FUNCTION contact_check_claim_complete_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.state = 'completed' AND OLD.state IS DISTINCT FROM 'completed' THEN
        IF NOT EXISTS (
            SELECT 1 FROM events
             WHERE community_id = NEW.community_id
               AND id = NEW.receipt_id
               AND kind = 46043
               AND pubkey = NEW.contact_pubkey
               AND deleted_at IS NULL
               AND tags @> jsonb_build_array(
                       jsonb_build_array('e', encode(NEW.decision_id, 'hex')))
        ) THEN
            RAISE EXCEPTION 'contact claim completion lacks its receipt'
                USING ERRCODE = 'check_violation';
        END IF;
    END IF;
    RETURN NULL;
END
$$;
CREATE CONSTRAINT TRIGGER contact_check_claim_complete_v1
    AFTER UPDATE ON contact_claims DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION contact_check_claim_complete_v1();

-- 6. Soft-deleting the original fences every still-open claim in the same
--    statement: pending dies unclaimed, claimed/started become cancelled,
--    unknown stays fenced. Runs on the partitioned parent so every leaf —
--    present and future — inherits it.
CREATE FUNCTION contact_cancel_claim_on_original_delete_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.kind = 9 AND OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL THEN
        UPDATE contact_claims
           SET state = 'cancelled'
         WHERE community_id = OLD.community_id
           AND original_id = OLD.id
           AND state IN ('pending', 'claimed', 'started', 'unknown');
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER contact_cancel_claim_on_original_delete_v1
    BEFORE UPDATE ON events
    FOR EACH ROW EXECUTE FUNCTION contact_cancel_claim_on_original_delete_v1();

-- 7. Deferred reciprocal guards — production catalog for the R4 contract.
--    Same checks as the reviewed fixture, `_v1` names, plus the pinned canvas
--    tag and the claim row.
CREATE FUNCTION contact_assert_quota_v1(p_community uuid, p_stripe smallint)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    recorded integer;
    actual bigint;
BEGIN
    SELECT used INTO recorded FROM contact_quota
        WHERE community_id = p_community AND stripe = p_stripe;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'contact quota stripe missing' USING ERRCODE = 'check_violation';
    END IF;
    SELECT count(*) INTO actual FROM contact_routes
        WHERE community_id = p_community AND stripe = p_stripe;
    IF recorded <> actual THEN
        RAISE EXCEPTION 'contact quota does not match routed rows'
            USING ERRCODE = 'check_violation';
    END IF;
END
$$;

CREATE FUNCTION contact_assert_route_v1(p_community uuid, p_original bytea)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    r contact_routes%ROWTYPE;
BEGIN
    SELECT * INTO r FROM contact_routes
        WHERE community_id = p_community AND original_id = p_original;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'routed original has no route' USING ERRCODE = 'check_violation';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM events WHERE community_id = r.community_id AND id = r.original_id
            AND created_at = r.original_created_at AND channel_id = r.channel_id
            AND kind = 9 AND contact_class = 1
    ) THEN
        RAISE EXCEPTION 'route has no matching classified original'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM events WHERE community_id = r.community_id AND id = r.decision_id
            AND created_at = r.decision_created_at AND channel_id = r.channel_id
            AND kind = 46044 AND pubkey = r.relay_pubkey AND deleted_at IS NULL
            AND tags @> jsonb_build_array(
                    jsonb_build_array('h', r.channel_id::text),
                    jsonb_build_array('p', encode(r.contact_pubkey, 'hex')),
                    jsonb_build_array('original', encode(r.original_id, 'hex')),
                    jsonb_build_array('phase', 'decision'))
            AND (r.canvas_id IS NULL OR tags @> jsonb_build_array(
                    jsonb_build_array('canvas', encode(r.canvas_id, 'hex'))))
    ) THEN
        RAISE EXCEPTION 'route has no matching decision proof'
            USING ERRCODE = 'check_violation';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM contact_claims
         WHERE community_id = r.community_id AND decision_id = r.decision_id
    ) THEN
        RAISE EXCEPTION 'routed decision has no claim row'
            USING ERRCODE = 'check_violation';
    END IF;
    PERFORM contact_assert_quota_v1(r.community_id, r.stripe);
END
$$;

CREATE FUNCTION contact_check_original_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    original bytea;
BEGIN
    IF NEW.kind = 9 AND NEW.contact_class = 1 THEN
        PERFORM contact_assert_route_v1(NEW.community_id, NEW.id);
    ELSIF NEW.kind = 46044 THEN
        SELECT original_id INTO original FROM contact_routes
            WHERE community_id = NEW.community_id AND decision_id = NEW.id;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'decision proof has no route'
                USING ERRCODE = 'check_violation';
        END IF;
        PERFORM contact_assert_route_v1(NEW.community_id, original);
    END IF;
    RETURN NULL;
END
$$;

CREATE FUNCTION contact_check_proof_delete_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.kind = 46044 AND EXISTS (
        SELECT 1 FROM contact_routes
         WHERE community_id = OLD.community_id AND decision_id = OLD.id
    ) THEN
        RAISE EXCEPTION 'cannot remove proof while retaining its route'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END
$$;

CREATE FUNCTION contact_check_route_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM contact_assert_route_v1(NEW.community_id, NEW.original_id);
    RETURN NULL;
END
$$;
CREATE CONSTRAINT TRIGGER contact_check_route_v1
    AFTER INSERT ON contact_routes DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION contact_check_route_v1();

CREATE FUNCTION contact_check_quota_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM contact_assert_quota_v1(NEW.community_id, NEW.stripe);
    RETURN NULL;
END
$$;
CREATE CONSTRAINT TRIGGER contact_check_quota_v1
    AFTER INSERT OR UPDATE ON contact_quota DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION contact_check_quota_v1();

-- Route, quota, and claim rows are evidence: no DELETE except the fenced
-- whole-community purge executor — the same exemption
-- contact_guard_original_v1 grants kind-9 originals and 46044 proofs.
CREATE FUNCTION contact_purge_executor_exempt_v1(p_community uuid)
RETURNS boolean LANGUAGE plpgsql STABLE AS $$
DECLARE
    executor_community TEXT;
    executor_generation TEXT;
    lifecycle TEXT;
    expected_generation BIGINT;
BEGIN
    executor_community := current_setting('buzz.deletion_executor_community', true);
    executor_generation := current_setting('buzz.deletion_fence_generation', true);
    SELECT deletion_state, deletion_fence_generation
      INTO lifecycle, expected_generation
      FROM communities
     WHERE id = p_community;
    RETURN executor_community = p_community::TEXT
       AND executor_generation ~ '^[0-9]+$'
       AND executor_generation::BIGINT = expected_generation
       AND lifecycle IN ('fenced', 'tombstone');
END
$$;

CREATE FUNCTION contact_route_no_delete_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF contact_purge_executor_exempt_v1(OLD.community_id) THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'contact decision routes are never deleted'
        USING ERRCODE = 'check_violation';
END
$$;
CREATE TRIGGER contact_route_no_delete_v1
    BEFORE DELETE ON contact_routes
    FOR EACH ROW EXECUTE FUNCTION contact_route_no_delete_v1();

CREATE FUNCTION contact_quota_no_delete_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF contact_purge_executor_exempt_v1(OLD.community_id) THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'contact quota rows are never deleted'
        USING ERRCODE = 'check_violation';
END
$$;
CREATE TRIGGER contact_quota_no_delete_v1
    BEFORE DELETE ON contact_quota
    FOR EACH ROW EXECUTE FUNCTION contact_quota_no_delete_v1();

-- 8. Leaf-guard installer + catalog verification. Postgres does not propagate
--    constraint triggers from the partitioned parent to leaves, so every
--    events leaf — existing or created later by partition maintenance — must
--    carry the deferred pair explicitly. `contact_verify_catalog_v1` is the
--    fail-closed check the decision path runs in the decision transaction.
CREATE FUNCTION contact_install_leaf_guards_v1(leaf regclass) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_inherits
         WHERE inhparent = 'events'::regclass AND inhrelid = leaf
    ) THEN
        RAISE EXCEPTION 'contact leaf guards apply only to events partitions: %', leaf
            USING ERRCODE = 'wrong_object_type';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgrelid = leaf AND tgname = 'contact_check_original_v1'
           AND NOT tgisinternal
    ) THEN
        EXECUTE format(
            'CREATE CONSTRAINT TRIGGER contact_check_original_v1 AFTER INSERT ON %s '
            'DEFERRABLE INITIALLY DEFERRED FOR EACH ROW '
            'EXECUTE FUNCTION contact_check_original_v1()', leaf);
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgrelid = leaf AND tgname = 'contact_check_proof_delete_v1'
           AND NOT tgisinternal
    ) THEN
        EXECUTE format(
            'CREATE CONSTRAINT TRIGGER contact_check_proof_delete_v1 AFTER DELETE ON %s '
            'DEFERRABLE INITIALLY DEFERRED FOR EACH ROW '
            'EXECUTE FUNCTION contact_check_proof_delete_v1()', leaf);
    END IF;
END
$$;

CREATE FUNCTION contact_verify_catalog_v1() RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    uncovered text;
BEGIN
    SELECT string_agg(leaf::text, ', ' ORDER BY leaf::text) INTO uncovered
      FROM (
        SELECT inhrelid::regclass AS leaf
          FROM pg_inherits
         WHERE inhparent = 'events'::regclass
        EXCEPT
        SELECT tgrelid::regclass FROM pg_trigger
         WHERE tgname = 'contact_check_original_v1' AND NOT tgisinternal
        EXCEPT
        SELECT tgrelid::regclass FROM pg_trigger
         WHERE tgname = 'contact_check_proof_delete_v1' AND NOT tgisinternal
      ) missing;
    IF uncovered IS NOT NULL THEN
        RAISE EXCEPTION 'contact routing catalog incomplete on partitions: %', uncovered
            USING ERRCODE = 'feature_not_supported';
    END IF;
END
$$;

DO $$
DECLARE
    leaf regclass;
BEGIN
    FOR leaf IN
        SELECT inhrelid::regclass FROM pg_inherits WHERE inhparent = 'events'::regclass
    LOOP
        PERFORM contact_install_leaf_guards_v1(leaf);
    END LOOP;
END
$$;

-- 9. Claims are tenant-scoped evidence: same community write fence as the
--    route/quota relations.
SELECT attach_community_write_fence('contact_claims');
