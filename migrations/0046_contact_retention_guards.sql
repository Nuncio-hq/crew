-- Contact retention foundation (issue #355).
--
-- This migration records the storage boundary without enabling automatic
-- contact routing. Existing writers that do not provide a reviewed contact
-- decision are classified as suppressed (0). A later routing migration may
-- add an atomic decision/proof path; it must not weaken these guards.
SET LOCAL lock_timeout = '5s';

ALTER TABLE events ADD COLUMN contact_class SMALLINT;
ALTER TABLE events ADD CONSTRAINT contact_class_shape
    CHECK (contact_class IS NULL OR (kind = 9 AND contact_class BETWEEN 0 AND 12));

-- A kind-9 row is a signed original. Its storage identity and classification
-- are immutable after INSERT. Legacy rows remain NULL and cannot be promoted;
-- old writers inserting a new kind-9 row receive the explicit suppressed
-- marker so an absent routing decision never becomes an implicit route.
CREATE FUNCTION contact_classify_original_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.kind = 9 THEN
            IF NEW.contact_class IS NULL THEN
                NEW.contact_class := 0;
            ELSIF NEW.contact_class <> 0 THEN
                RAISE EXCEPTION 'contact routing classification requires the reviewed decision path'
                    USING ERRCODE = 'check_violation';
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

CREATE TRIGGER contact_classify_original_v1
    BEFORE INSERT OR UPDATE ON events
    FOR EACH ROW EXECUTE FUNCTION contact_classify_original_v1();

-- Ordinary event updates/deletes must not erase a signed original. The only
-- current hard-delete exception is the existing, fenced whole-community
-- executor: it proves the same community and generation that owns the fence.
-- No free-standing GUC or age value authorizes a per-message DELETE.
CREATE FUNCTION contact_guard_original_v1() RETURNS trigger
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
        RETURN NEW;
    END IF;

    IF OLD.kind <> 9 THEN
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

    RAISE EXCEPTION 'ordinary hard deletion of kind-9 originals is forbidden'
        USING ERRCODE = 'check_violation';
END
$$;

CREATE TRIGGER contact_guard_original_v1
    BEFORE UPDATE OR DELETE ON events
    FOR EACH ROW EXECUTE FUNCTION contact_guard_original_v1();

-- Durable decision evidence. This is storage only: no producer, relay proof
-- kind, resolver, or GC worker is enabled by this migration. The server stamp
-- is written by a trigger even when a caller supplies a stale value.
CREATE TABLE contact_routes (
    community_id        UUID NOT NULL REFERENCES communities(id),
    original_id         BYTEA NOT NULL,
    original_created_at TIMESTAMPTZ NOT NULL,
    channel_id          UUID NOT NULL,
    contact_pubkey      BYTEA NOT NULL,
    relay_pubkey        BYTEA NOT NULL,
    decision_id         BYTEA NOT NULL,
    decision_created_at TIMESTAMPTZ NOT NULL,
    stripe              SMALLINT NOT NULL CHECK (stripe BETWEEN 0 AND 15),
    decided_at          TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (community_id, original_id),
    UNIQUE (community_id, decision_id),
    CHECK (octet_length(original_id) = 32),
    CHECK (octet_length(contact_pubkey) = 32),
    CHECK (octet_length(relay_pubkey) = 32),
    CHECK (octet_length(decision_id) = 32)
);
CREATE INDEX contact_routes_stripe
    ON contact_routes (community_id, stripe);

CREATE TABLE contact_quota (
    community_id UUID NOT NULL REFERENCES communities(id),
    stripe       SMALLINT NOT NULL,
    used         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (community_id, stripe),
    CHECK (stripe BETWEEN 0 AND 15),
    CHECK (used BETWEEN 0 AND 8192)
);

CREATE FUNCTION contact_stamp_decision_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.decided_at := clock_timestamp();
    RETURN NEW;
END
$$;

CREATE TRIGGER contact_stamp_decision_v1
    BEFORE INSERT ON contact_routes
    FOR EACH ROW EXECUTE FUNCTION contact_stamp_decision_v1();

CREATE FUNCTION contact_route_immutable_v1() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW IS DISTINCT FROM OLD THEN
        RAISE EXCEPTION 'contact decision evidence is immutable'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER contact_route_immutable_v1
    BEFORE UPDATE ON contact_routes
    FOR EACH ROW EXECUTE FUNCTION contact_route_immutable_v1();

-- These relations are tenant-scoped and are included in the reviewed
-- child-before-parent deletion order. The deletion catalog therefore keeps
-- the storage slice fail-closed if either relation or its write fence drifts.
SELECT attach_community_write_fence('contact_routes');
SELECT attach_community_write_fence('contact_quota');
