# Phase 2 — Cross-process lease and ACP session revalidation

## Status

**Implemented / verified** on current `origin/main` (2026-08-05). Shared lease is
acquired before ensure/create/reattach; recreate/reattach forces session
self-heal. See [README.md](README.md). Phase 5 remains deferred.

## Goal

Make active-use detection authoritative and close the race between an ACP prompt
and `git worktree remove`.

An active Project turn must prevent eviction. An idle cached ACP session must not
keep disk forever, but it must never be reused after its checkout was evicted
without revalidation and fresh `session/new`.

## Shared crate

Add `crates/buzz-worktree` and include it in the root workspace. Add path
dependencies from:

- `crates/buzz-acp/Cargo.toml`;
- `desktop/src-tauri/Cargo.toml`.

The crate owns:

- canonical lease/record path derivation under the common Git directory;
- a validated full-root lease key;
- non-blocking shared active lease acquisition;
- non-blocking exclusive eviction lease acquisition;
- typed `Busy`, `InvalidIdentity`, `UnsupportedVersion`, and I/O errors;
- RAII release on every return/panic path.

Select a maintained cross-platform advisory-lock dependency and run the repo's
dependency/license policy before implementation proceeds. Do not build stale PID
recovery or process probing by hand. The lease file contains no credentials or
message content.

## ACP flow

Extend `SessionState` in `crates/buzz-acp/src/pool.rs` with a Project workspace
binding keyed by conversation/thread identity. The binding records at least root,
path, branch, common Git directory, and observed eviction generation.

Before every Project-thread prompt, including an existing ACP session:

1. resolve the trusted root workspace authority using the existing owner/root
   rules;
2. acquire a shared active lease;
3. verify the registered checkout, common Git directory, expected branch, root
   claim, and generation;
4. if the existing binding is valid, reuse the ACP session;
5. if missing, evicted, or generation-changed, invalidate that conversation's ACP
   session, call the deterministic ensure/reattach path, then run a fresh
   `session/new` with the verified cwd;
6. retain the shared lease until the prompt completes or fails.

Keep ordinary chats, DMs, heartbeats, and non-Project sessions unchanged.
Provision failure remains fail-closed; never fall back to the source checkout.

Clear the workspace binding in every path that currently clears its session:
`invalidate_channel`, routing-channel invalidation, agent exit, model rotation,
and `invalidate_all`.

## Cleanup flow

In `project_worktree_cleanup.rs`, after existing repo/path/branch validation:

1. acquire the non-blocking exclusive eviction lease;
2. if busy, return typed `refused` without running Git mutation;
3. while holding exclusive ownership, re-run worktree registration, branch,
   status, and ignored/protected-state checks;
4. remove/prune;
5. atomically advance the eviction generation;
6. release the lease.

The check and mutation must share one exclusive lease. A frontend `activeTurns`
check remains useful copy but is not part of authorization.

## Tests

`buzz-worktree` (verified):

- [x] multiple shared holders can coexist;
- [x] exclusive acquisition refuses while any shared holder exists;
- [x] shared acquisition refuses while exclusive holder exists;
- [x] locks release on drop;
- [x] invalid root and unsupported metadata versions fail closed;
- [x] a subprocess-based test proves cross-process behavior, not merely two threads.

`buzz-acp` (verified):

- [x] active turn holds the lease through prompt completion and all error exits;
- [x] an unchanged existing workspace reuses the session;
- [x] an evicted/generation-changed workspace invalidates the old session, reattaches
  the branch, and creates a fresh session;
- [x] non-Project paths acquire no worktree lease;
- [x] routing-channel and all-session invalidation clear bindings;
- [x] exclusive lease before prompt prevents ensure/create mutation;
- [x] start/remove race converges to refusal or safe reattach, never missing cwd;
- [x] recreate/reattach invalidates cached session even when generation strings match.

Desktop Tauri (verified):

- [x] removal returns `refused` while a fixture holds a shared lease;
- [x] no Git remove command executes on refusal;
- [x] removal succeeds after the holder exits;
- [x] thread-panel remove uses the same lease/generation protocol.

## Exit gate

- [x] There is no interval in which cleanup can pass authorization while an ACP turn
  begins using the same workspace (lease before ensure).
- [x] Cached sessions self-heal after eviction / recreate.
- [x] The new crate and focused ACP/Desktop tests pass; MSRV 1.88 compile/test for
  `buzz-worktree` verified.

## Rollback

Disable destructive controls if the lease crate must be reverted. Do not fall
back to observer-only authorization.
