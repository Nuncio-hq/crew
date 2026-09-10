CREATE UNIQUE INDEX unresolved_managed_agent_delete
    ON operations(resource_key)
    WHERE kind = 'managed-agent-delete' AND reconciled = 0;
PRAGMA user_version = 2;
