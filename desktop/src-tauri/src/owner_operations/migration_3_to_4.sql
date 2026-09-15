-- Schema v4 lets a persona deletion coordinator and its first child share the
-- child's managed-agent resource claim.  The old indexes remain correct for
-- every direct deletion and for unrelated operations, but reject that one
-- intentionally paired set.  Rebuild both indexes in the opener's existing
-- immediate migration transaction so a crash rolls back the whole change.
DROP INDEX IF EXISTS unresolved_resource;
CREATE UNIQUE INDEX unresolved_resource
    ON operations(
        owner,
        community,
        kind,
        CASE
            WHEN kind = 'managed-agent-delete' AND json_valid(record_json) THEN
                CASE
                    WHEN json_type(record_json, '$.payload.cascade_parent') = 'text'
                    THEN resource_key || ':' || json_extract(record_json, '$.payload.cascade_parent')
                    ELSE resource_key
                END
            ELSE resource_key
        END
    )
    WHERE reconciled = 0;

DROP INDEX IF EXISTS unresolved_managed_agent_delete;
CREATE UNIQUE INDEX unresolved_managed_agent_delete
    ON operations(
        CASE
            WHEN json_valid(record_json) THEN
                CASE
                    WHEN json_type(record_json, '$.payload.cascade_parent') = 'text'
                    THEN resource_key || ':' || json_extract(record_json, '$.payload.cascade_parent')
                    ELSE resource_key
                END
            ELSE resource_key
        END
    )
    WHERE kind = 'managed-agent-delete' AND reconciled = 0;

PRAGMA user_version = 4;
