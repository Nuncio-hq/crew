-- Fixture-only retention delta. Never loaded by migrations or serving startup.
-- Proposed age anchor: server-stamped immutable route.decided_at. Original and
-- signed proof created_at values are NOT retention clocks.
CREATE FUNCTION contact_retention_now() RETURNS timestamptz
LANGUAGE sql VOLATILE AS $$ SELECT clock_timestamp() $$;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM contact_routes) THEN
        RAISE EXCEPTION 'fixture retention install requires an empty route table';
    END IF;
END $$;
DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = 'contact_routes'::regclass
          AND attname = 'decided_at'
          AND NOT attisdropped
    ) THEN
        ALTER TABLE contact_routes ADD COLUMN decided_at timestamptz NOT NULL
            DEFAULT contact_retention_now();
    END IF;
END $$;

CREATE FUNCTION contact_stamp_decision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    -- Override even an explicitly supplied value: the caller cannot backdate
    -- newly recorded evidence. Existing route immutability protects this field.
    NEW.decided_at := contact_retention_now();
    RETURN NEW;
END $$;
CREATE TRIGGER contact_stamp_decision BEFORE INSERT ON contact_routes
    FOR EACH ROW EXECUTE FUNCTION contact_stamp_decision();

CREATE FUNCTION contact_guard_replay_floor() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE retention_clock timestamptz := contact_retention_now();
BEGIN
    IF OLD.contact_replay_floor IS NOT NULL AND
        (NEW.contact_replay_floor IS NULL OR NEW.contact_replay_floor < OLD.contact_replay_floor) THEN
        RAISE EXCEPTION 'contact replay floor cannot decrease' USING ERRCODE='23514';
    END IF;
    IF NEW.contact_replay_floor IS DISTINCT FROM OLD.contact_replay_floor AND
        (retention_clock IS NULL OR NEW.contact_replay_floor > retention_clock - interval '90 days') THEN
        RAISE EXCEPTION 'contact replay floor must cover old history only' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER contact_guard_replay_floor BEFORE UPDATE ON communities
    FOR EACH ROW EXECUTE FUNCTION contact_guard_replay_floor();

CREATE FUNCTION contact_require_retention(r contact_routes) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE
    retention_clock timestamptz := contact_retention_now();
    floor_time timestamptz;
BEGIN
    IF retention_clock IS NULL OR r.decided_at IS NULL OR
        r.decided_at > retention_clock - interval '90 days' THEN
        RAISE EXCEPTION 'contact decision has not been retained for ninety days' USING ERRCODE='23514';
    END IF;
    SELECT contact_replay_floor INTO floor_time FROM communities WHERE id=r.community_id;
    IF floor_time IS NULL OR r.original_created_at > floor_time THEN
        RAISE EXCEPTION 'contact GC lacks durable original replay coverage' USING ERRCODE='23514';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM events WHERE community_id=r.community_id
        AND id=r.original_id AND created_at=r.original_created_at AND kind=9 AND contact_class=1) THEN
        RAISE EXCEPTION 'contact GC must preserve its classified original' USING ERRCODE='23514';
    END IF;
END $$;

CREATE FUNCTION contact_guard_retention_event() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r contact_routes%ROWTYPE;
BEGIN
    IF TG_OP='UPDATE' THEN
        -- Legacy NULL classification is still an original: do not let an
        -- identity rewrite escape the kind9 hard-delete/replay boundary.
        IF OLD.kind=9 AND
            ROW(OLD.community_id,OLD.id,OLD.created_at,OLD.kind) IS DISTINCT FROM
            ROW(NEW.community_id,NEW.id,NEW.created_at,NEW.kind) THEN
            RAISE EXCEPTION 'original identity is immutable, including legacy' USING ERRCODE='23514';
        END IF;
        IF (OLD.kind=46044 OR NEW.kind=46044) AND NEW IS DISTINCT FROM OLD THEN
            RAISE EXCEPTION 'contact proof data is immutable' USING ERRCODE='23514';
        END IF;
        RETURN NEW;
    END IF;
    IF OLD.kind=9 THEN
        -- No age/floor/GUC exception: positive physical original erasure and
        -- the real fenced community executor require separately reviewed seams.
        RAISE EXCEPTION 'ordinary raw original hard deletion is forbidden' USING ERRCODE='23514';
    ELSIF OLD.kind=46044 THEN
        SELECT * INTO r FROM contact_routes WHERE community_id=OLD.community_id
            AND decision_id=OLD.id AND decision_created_at=OLD.created_at;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'untracked proof deletion is forbidden' USING ERRCODE='23514';
        END IF;
        PERFORM contact_require_retention(r);
        PERFORM contact_assert_route(r.community_id,r.original_id);
    END IF;
    RETURN OLD;
END $$;
CREATE TRIGGER contact_guard_retention_event BEFORE UPDATE OR DELETE ON events
    FOR EACH ROW EXECUTE FUNCTION contact_guard_retention_event();

CREATE FUNCTION contact_guard_route_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM contact_require_retention(OLD);
    RETURN OLD;
END $$;
CREATE TRIGGER contact_guard_route_delete BEFORE DELETE ON contact_routes
    FOR EACH ROW EXECUTE FUNCTION contact_guard_route_delete();

CREATE FUNCTION contact_check_proof_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.kind=46044 AND EXISTS (SELECT 1 FROM contact_routes
        WHERE community_id=OLD.community_id AND decision_id=OLD.id) THEN
        RAISE EXCEPTION 'GC cannot remove proof while retaining its route' USING ERRCODE='23514';
    END IF;
    RETURN NULL;
END $$;
DO $$ DECLARE leaf record; BEGIN
    FOR leaf IN SELECT inhrelid FROM pg_inherits WHERE inhparent='events'::regclass LOOP
        EXECUTE format('CREATE CONSTRAINT TRIGGER contact_check_proof_delete
            AFTER DELETE ON %s DEFERRABLE INITIALLY DEFERRED
            FOR EACH ROW EXECUTE FUNCTION contact_check_proof_delete()',leaf.inhrelid::regclass);
    END LOOP;
END $$;

CREATE FUNCTION contact_check_route_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM events WHERE community_id=OLD.community_id
        AND id=OLD.decision_id AND created_at=OLD.decision_created_at) THEN
        RAISE EXCEPTION 'GC cannot remove route while retaining its proof' USING ERRCODE='23514';
    END IF;
    PERFORM contact_assert_quota(OLD.community_id,OLD.stripe);
    RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER contact_check_route_delete AFTER DELETE ON contact_routes
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION contact_check_route_delete();
