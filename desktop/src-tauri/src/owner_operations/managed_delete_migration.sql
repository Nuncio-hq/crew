-- Managed-agent deletion is an app-global claim. A workspace switch must not
-- allow the same local instance to be restored or deleted a second time while
-- its original operation still has unresolved side effects.
CREATE UNIQUE INDEX unresolved_managed_agent_delete
    ON operations(resource_key)
    WHERE kind = 'managed-agent-delete' AND reconciled = 0;
PRAGMA user_version = 2;
