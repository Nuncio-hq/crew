-- Schema v3 adds the direct Wiki successor relation. The managed-agent claim
-- index is repeated here so journals created by either pre-merge v2 branch
-- converge on the same v3 schema before the opener commits user_version.
CREATE UNIQUE INDEX IF NOT EXISTS unresolved_managed_agent_delete
    ON operations(resource_key)
    WHERE kind = 'managed-agent-delete' AND reconciled = 0;

CREATE TABLE IF NOT EXISTS wiki_publication_successors (
    owner TEXT NOT NULL,
    community TEXT NOT NULL,
    resource_key TEXT NOT NULL CHECK (length(resource_key) BETWEEN 1 AND 512),
    predecessor_id TEXT NOT NULL,
    predecessor_revision INTEGER NOT NULL CHECK (predecessor_revision >= 0),
    successor_id TEXT NOT NULL,
    created_at INTEGER NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (owner, community, predecessor_id),
    UNIQUE (owner, community, successor_id)
);
CREATE INDEX IF NOT EXISTS wiki_successor_by_owner
    ON wiki_publication_successors(owner, community, successor_id);
PRAGMA user_version = 3;
