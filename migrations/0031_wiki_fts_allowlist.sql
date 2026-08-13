-- Crew Wiki (#200): index kind 30023 (company wiki / NIP-23) and 30623
-- (repo wiki pages + TOC) on fresh-install positive allowlists.
--
-- Brownfield databases keep the exclusion CASE from schema.sql / 0001+0005+0009
-- and already FTS-index these kinds. Do not add 30623 to the privacy exclusion
-- CASE. This migration only rewrites the *allowlist* expression when present.

DO $$
DECLARE
    existing_expression TEXT;
BEGIN
    SELECT pg_get_expr(d.adbin, d.adrelid)
      INTO existing_expression
      FROM pg_attrdef d
      JOIN pg_attribute a
        ON a.attrelid = d.adrelid
       AND a.attnum = d.adnum
     WHERE d.adrelid = 'events'::regclass
       AND a.attname = 'search_tsv';

    IF existing_expression IS NULL THEN
        RAISE EXCEPTION 'events.search_tsv generated expression not found';
    END IF;

    -- Only extend the fresh-install positive allowlist. Exclusion-CASE
    -- installations already index 30023/30623 (they are not in the skip set).
    IF existing_expression LIKE '%kind IN (0, 9, 40002, 45001, 45003)%'
       AND existing_expression NOT LIKE '%30623%' THEN
        ALTER TABLE events DROP COLUMN search_tsv;
        ALTER TABLE events ADD COLUMN search_tsv TSVECTOR GENERATED ALWAYS AS (
            CASE WHEN kind IN (0, 9, 40002, 45001, 45003, 30023, 30623)
                 THEN to_tsvector('simple', content)
                 ELSE NULL::tsvector
            END
        ) STORED;
        CREATE INDEX idx_events_search_tsv ON events USING GIN (search_tsv);
    END IF;
END $$;
