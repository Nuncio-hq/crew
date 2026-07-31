# Phase 01 — Worktree base from origin + lifecycle commands

- **Status:** Not started
- **Priority:** high — this is the bug that corrupted the v0.0.5 test session

## Context

`crates/buzz-acp/src/thread_workspace.rs:88` resolves the new worktree's base
with `git rev-parse HEAD` against the source checkout and never fetches. When
the user's local `main` lags the remote, every agent in that thread edits stale
code and its PR conflicts on merge. This happened live: the source checkout sat
4 commits behind while agents worked on `a152f59b1`.

There is also no removal path anywhere in the repo — each thread leaves a
worktree and a branch forever.

## Requirements

1. Before creating a worktree, fetch the remote and resolve the base from
   `origin/<default-branch>`.
2. Never block agent startup on a failed fetch. Fall back to local `HEAD` and
   report that the base is stale, with the distance from the remote tip.
3. Expose the distance so the UI can show it (Phase 02 renders
   `N behind origin/main`).
4. Add worktree/branch/PR teardown the UI can call.

## Files

- `crates/buzz-acp/src/thread_workspace.rs` — base resolution, `ThreadWorkspace`
- `crates/buzz-acp/src/` — whichever module already publishes the
  `thread_workspace_ready` observer payload (extend it, do not add a new event)
- `desktop/src-tauri/src/commands/` — new Tauri commands for teardown
- `desktop/src/shared/api/agentControl.ts` + `desktop/src/shared/api/types.ts`

## Steps

1. Resolve the default branch from the remote rather than assuming `main`
   (`git remote show origin` or `git symbolic-ref refs/remotes/origin/HEAD`).
   Cache it per repo root for the process; do not shell out per thread.
2. `git fetch origin --quiet` with a bounded timeout. On success the base is
   `origin/<default>`; on failure keep `HEAD` and mark the result stale.
3. Extend `ThreadWorkspace` with the fields the UI needs:
   `base_revision` (unchanged), `base_source` (`remote` | `local-fallback`), and
   `commits_behind_remote` (`git rev-list --count HEAD..origin/<default>`, `0`
   when the fetch failed and the number is unknown — use `Option`, not a
   sentinel).
4. Thread those fields through the `thread_workspace_ready` payload so
   `projectThreadWorkspaceStore.ts` can project them.
5. Add teardown commands, each returning a typed refusal rather than an `Err`
   string the UI has to parse:
   - `remove_thread_worktree` — refuses when `git status --porcelain` is
     non-empty in that worktree; otherwise `git worktree remove` + `git worktree prune`.
   - `delete_thread_branch` — refuses while the branch is checked out anywhere.
   - `close_thread_pull_request` — `gh pr close` for the branch's PR.
6. Unit-test the base resolution: remote ahead, fetch failure, detached remote
   HEAD, and a repo whose default branch is not `main`.

## Validation

```bash
cargo test -p buzz-acp
cargo test --manifest-path desktop/src-tauri/Cargo.toml
just ci
```

Manual: put the source checkout one commit behind `origin/main`, open a new
Project thread, confirm the worktree HEAD equals the remote tip.

## Risk

Fetching on thread start adds network latency to first agent turn. Keep the
timeout short and asynchronous relative to worktree creation; a slow network
must degrade to the local fallback, never to a hang.

## Rollback

Revert the commit. Existing worktrees keep their branch points either way —
base resolution only affects newly created worktrees.
