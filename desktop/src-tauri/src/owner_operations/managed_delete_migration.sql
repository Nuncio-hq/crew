-- Managed-agent deletion is an app-global claim. A workspace switch must not
-- allow the same local instance to be restored or deleted a second time while
-- its original operation still has unresolved side effects.
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
PRAGMA user_version = 2;
