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
-- Persona cascades reserve one coordinator plus one child for the first
-- managed-agent resource.  Keep direct-operation claims keyed by the resource
-- while giving a validated child its parent-qualified key; all other rows
-- retain the original uniqueness contract.
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
CREATE INDEX owner_retention ON operations(owner, reconciled, updated_at);
PRAGMA user_version = 1;
