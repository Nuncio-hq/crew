-- Schema v2 adds only the direct Wiki successor relation. Existing operation
-- rows, payloads, revisions, initial claims, and retention metadata remain
-- byte-for-byte untouched. This script is run inside BEGIN IMMEDIATE and is
-- idempotent so an interrupted opener can safely retry it.
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
PRAGMA user_version = 2;
