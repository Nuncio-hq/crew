-- #355 isolated R4 storage proof. Never loaded by migrations or startup.
-- Baseline without guards produced seven healthy REDs before adding them.
--
-- Desired-state test databases now carry the inert production storage guard
-- foundation. Remove only those production triggers in this disposable
-- fixture; the approved fixture guards below remain the contract under test.
DO $$
DECLARE
    relation_name regclass;
BEGIN
    FOR relation_name IN
        SELECT c.oid::regclass
        FROM pg_class c
        WHERE c.oid = 'events'::regclass
           OR c.oid IN (SELECT inhrelid FROM pg_inherits WHERE inhparent = 'events'::regclass)
    LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS contact_classify_original_v1 ON %s', relation_name);
        EXECUTE format('DROP TRIGGER IF EXISTS contact_guard_original_v1 ON %s', relation_name);
    END LOOP;
END
$$;
DROP TRIGGER IF EXISTS contact_stamp_decision_v1 ON contact_routes;

ALTER TABLE events ADD COLUMN IF NOT EXISTS contact_class SMALLINT;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'events'::regclass AND conname = 'contact_class_shape'
    ) THEN
        ALTER TABLE events ADD CONSTRAINT contact_class_shape
            CHECK (contact_class IS NULL OR (kind = 9 AND contact_class BETWEEN 0 AND 12));
    END IF;
END
$$;
CREATE TABLE IF NOT EXISTS contact_routes (
    community_id UUID NOT NULL REFERENCES communities(id),
    original_id BYTEA NOT NULL,
    original_created_at TIMESTAMPTZ NOT NULL,
    channel_id UUID NOT NULL,
    contact_pubkey BYTEA NOT NULL,
    relay_pubkey BYTEA NOT NULL,
    decision_id BYTEA NOT NULL,
    decision_created_at TIMESTAMPTZ NOT NULL,
    stripe SMALLINT NOT NULL,
    decided_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (community_id, original_id),
    UNIQUE (community_id, decision_id)
);
CREATE TABLE IF NOT EXISTS contact_quota (
    community_id UUID NOT NULL REFERENCES communities(id),
    stripe SMALLINT NOT NULL,
    used INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (community_id, stripe),
    CHECK (stripe BETWEEN 0 AND 15),
    CHECK (used BETWEEN 0 AND 8192)
);

-- GREEN implementation below, installed only in the disposable test database.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'events'::regclass AND conname = 'contact_class_shape'
    ) THEN
        ALTER TABLE events ADD CONSTRAINT contact_class_shape
            CHECK (contact_class IS NULL OR (kind = 9 AND contact_class BETWEEN 0 AND 12));
    END IF;
END
$$;
ALTER TABLE contact_routes ADD CONSTRAINT contact_route_shape
    CHECK (stripe BETWEEN 0 AND 15 AND octet_length(original_id)=32
        AND octet_length(contact_pubkey)=32 AND octet_length(relay_pubkey)=32
        AND octet_length(decision_id)=32);
CREATE INDEX IF NOT EXISTS contact_routes_stripe ON contact_routes (community_id, stripe);

CREATE FUNCTION contact_classify_original() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.kind = 9 AND NEW.contact_class IS NULL THEN
            NEW.contact_class := 0;
        END IF;
    ELSE
        IF OLD.contact_class IS DISTINCT FROM NEW.contact_class THEN
            RAISE EXCEPTION 'contact classification is immutable' USING ERRCODE='23514';
        END IF;
        IF OLD.contact_class IS NOT NULL AND
            ROW(OLD.community_id,OLD.id,OLD.created_at,OLD.kind) IS DISTINCT FROM
            ROW(NEW.community_id,NEW.id,NEW.created_at,NEW.kind) THEN
            RAISE EXCEPTION 'classified original identity is immutable' USING ERRCODE='23514';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER contact_classify_original
    BEFORE INSERT OR UPDATE ON events
    FOR EACH ROW EXECUTE FUNCTION contact_classify_original();

CREATE FUNCTION contact_assert_quota(p_community uuid, p_stripe smallint) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    recorded integer;
    actual bigint;
BEGIN
    SELECT used INTO recorded FROM contact_quota
        WHERE community_id=p_community AND stripe=p_stripe;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'contact quota stripe missing' USING ERRCODE='23514';
    END IF;
    SELECT count(*) INTO actual FROM contact_routes
        WHERE community_id=p_community AND stripe=p_stripe;
    IF recorded <> actual THEN
        RAISE EXCEPTION 'contact quota does not match routed rows' USING ERRCODE='23514';
    END IF;
END $$;

CREATE FUNCTION contact_assert_route(p_community uuid, p_original bytea) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    r contact_routes%ROWTYPE;
BEGIN
    SELECT * INTO r FROM contact_routes
        WHERE community_id=p_community AND original_id=p_original;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'routed original has no route' USING ERRCODE='23514';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM events WHERE community_id=r.community_id AND id=r.original_id
            AND created_at=r.original_created_at AND channel_id=r.channel_id
            AND kind=9 AND contact_class=1
    ) THEN
        RAISE EXCEPTION 'route has no matching classified original' USING ERRCODE='23514';
    END IF;
    -- Storage binding only: ingress must separately authenticate the configured
    -- relay signer and validate the full wire payload before this can ship.
    IF NOT EXISTS (
        SELECT 1 FROM events WHERE community_id=r.community_id AND id=r.decision_id
            AND created_at=r.decision_created_at AND channel_id=r.channel_id
            AND kind=46044 AND pubkey=r.relay_pubkey AND deleted_at IS NULL
            AND tags @> jsonb_build_array(
                jsonb_build_array('h',r.channel_id::text),
                jsonb_build_array('p',encode(r.contact_pubkey,'hex')),
                jsonb_build_array('original',encode(r.original_id,'hex')),
                jsonb_build_array('phase','decision'))
    ) THEN
        RAISE EXCEPTION 'route has no matching decision proof' USING ERRCODE='23514';
    END IF;
    PERFORM contact_assert_quota(r.community_id,r.stripe);
END $$;

CREATE FUNCTION contact_check_original() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    original bytea;
BEGIN
    IF NEW.kind=9 AND NEW.contact_class=1 THEN
        PERFORM contact_assert_route(NEW.community_id,NEW.id);
    ELSIF NEW.kind=46044 THEN
        SELECT original_id INTO original FROM contact_routes
            WHERE community_id=NEW.community_id AND decision_id=NEW.id;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'decision proof has no route' USING ERRCODE='23514';
        END IF;
        PERFORM contact_assert_route(NEW.community_id,original);
    END IF;
    RETURN NULL;
END $$;
-- PostgreSQL constraint triggers are attached explicitly to each leaf. Parent
-- INSERTs run the selected leaf trigger too; the functional test covers both.
DO $$
DECLARE
    leaf record;
BEGIN
    FOR leaf IN SELECT inhrelid FROM pg_inherits WHERE inhparent='events'::regclass LOOP
        EXECUTE format('CREATE CONSTRAINT TRIGGER contact_check_original
            AFTER INSERT ON %s DEFERRABLE INITIALLY DEFERRED
            FOR EACH ROW EXECUTE FUNCTION contact_check_original()',leaf.inhrelid::regclass);
    END LOOP;
END $$;

CREATE FUNCTION contact_route_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW IS DISTINCT FROM OLD THEN
        RAISE EXCEPTION 'contact route identity is immutable' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER contact_route_immutable BEFORE UPDATE ON contact_routes
    FOR EACH ROW EXECUTE FUNCTION contact_route_immutable();

CREATE FUNCTION contact_check_route() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM contact_assert_route(NEW.community_id,NEW.original_id);
    RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER contact_check_route AFTER INSERT ON contact_routes
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION contact_check_route();

CREATE FUNCTION contact_check_quota() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM contact_assert_quota(NEW.community_id,NEW.stripe);
    RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER contact_check_quota AFTER INSERT OR UPDATE ON contact_quota
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION contact_check_quota();
