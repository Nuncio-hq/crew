CREATE TABLE operations (
    owner TEXT NOT NULL,
    community TEXT NOT NULL,
    id TEXT NOT NULL,
    kind TEXT NOT NULL,
    resource_key TEXT NOT NULL,
    initial_digest BLOB NOT NULL CHECK (length(initial_digest) = 32),
    revision INTEGER NOT NULL CHECK (revision >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    status TEXT NOT NULL,
    reconciled INTEGER NOT NULL CHECK (reconciled IN (0, 1)),
    record_json TEXT NOT NULL,
    bytes INTEGER NOT NULL CHECK (bytes = length(CAST(record_json AS BLOB))),
    PRIMARY KEY (owner, community, id)
);
CREATE UNIQUE INDEX unresolved_resource
    ON operations(owner, community, kind, resource_key)
    WHERE reconciled = 0;
CREATE INDEX owner_retention ON operations(owner, reconciled, updated_at);
CREATE UNIQUE INDEX unresolved_managed_agent_delete
    ON operations(resource_key)
    WHERE kind = 'managed-agent-delete' AND reconciled = 0;
PRAGMA user_version = 2;
