# Phase 3 — Durable channel identity and real last-used time

## Status

**Implemented / verified** on current `origin/main` (2026-08-05). See
[README.md](README.md). Phase 5 remains deferred.

## Goal

Stop inferring worktree ownership and idleness from a paginated UI window and Git
commit timestamps.

## Record format

Add a versioned record API to `buzz-worktree`. Store one record per full thread
root under the repository's common Git directory, separate from but consistent
with `buzz-thread-workspace-roots`.

V1 fields:

- `version`;
- `root_event_id` (64 hex);
- `routing_channel_id`;
- normalized relay/community scope when available from ACP configuration;
- `branch`;
- canonical `worktree_path`;
- `created_at`;
- `last_used_at`;
- `eviction_generation`.

Do not store prompt text, user identity secrets, credentials, or file contents.
Use same-directory temp writes, file sync, atomic rename, and a lock around
read-modify-write. Concurrent agent handoffs update `last_used_at` monotonically
(max timestamp), never backwards.

## ACP ownership

When `resolve_thread_session_workspace` has verified owner-authored Project
metadata and the real routing channel:

1. create/adopt the record;
2. reject any existing record whose full root, branch, common Git identity, or
   channel scope conflicts;
3. update `last_used_at` when a turn acquires its active lease;
4. keep creation/adoption idempotent across several agents.

Legacy worktrees are adopted only through this trusted ACP path. The Desktop UI
must not manufacture a channel binding from currently loaded messages.

## Registry projection

Extend `ProjectWorktreeEntry` and TypeScript bindings with typed local lifecycle
state, for example:

- `routingChannelId: string | null`;
- `createdAt: number | null`;
- `lastUsedAt: number | null`;
- `lifecycleIdentity: "verified" | "legacy" | "conflict"`.

`get_project_worktree_registry` joins Git worktree/branch truth with the durable
record. Git remains authoritative for current path/branch/head; the record is
authoritative for channel and use lifecycle.

Conflict or malformed records are visible but fail closed and are never
actionable.

## Channel buckets

Update `bucketWorktrees`:

- exact routing channel match → eligible for normal Active/Idle classification;
- different verified routing channel → `Other channels`, read-only;
- missing record → `Legacy / channel unknown`, read-only;
- conflict → `Needs attention`, read-only;
- active lease or open PR → Active;
- clean, no open PR, and verified `lastUsedAt` older than the threshold → Idle;
- missing `lastUsedAt` never means idle.

Remove commit age from lifecycle classification. Keep `lastCommitAt` only as Git
information in expanded details.

## Tests

Shared crate (verified):

- [x] atomic round-trip and version rejection;
- [x] concurrent monotonic last-use updates;
- [x] full-root prefix collision cannot alias records;
- [x] conflicting channel/branch adoption fails closed;
- [x] malformed/truncated record never authorizes deletion;
- [x] misnamed record body root / invalid routing channel fail closed.

ACP (verified via resolve_and_bind / binding tests):

- [x] trusted first turn creates the correct channel binding;
- [x] handoff/reuse updates last use without changing identity when unchanged;
- [x] generation advance / recreate self-heal invalidate stale sessions.

Desktop/frontend (verified):

- [x] verified same-channel, other-channel, legacy, and conflict projections;
- [x] idle uses durable `lastUsedAt` when verified;
- [x] a missing timeline root does not alter durable channel classification
  (channel-unknown remains non-actionable).

## Exit gate

- [x] The drawer remains correctly channel-scoped with only the newest message
  window loaded.
- [x] Every actionable reclaim path requires verified durable channel identity in
  Rust as well as UI.
- [x] Idle means actual ACP inactivity when a verified record exists, not old Git
  history alone.

## Migration and rollback

No eager migration. Existing entries remain legacy/read-only until a trusted turn
adopts them. Old binaries ignore the additive records. New binaries never delete
records merely because a worktree checkout was evicted; the retained record is
required for reattach and generation checks.
