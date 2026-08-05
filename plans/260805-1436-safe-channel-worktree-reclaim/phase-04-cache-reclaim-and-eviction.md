# Phase 4 — Cache-first reclaim and checkout eviction UX

## Status

**Implemented / verified** on current `origin/main` (2026-08-05). Preview returns
explicit cache/eviction capability fields; Rust requires
`expectedRoutingChannelId` for mutations. See [README.md](README.md). Phase 5
remains deferred / follow-up.

## Goal

Recover the generated data that dominates worktree disk usage without treating
source deletion as the first or only cleanup tool.

## Backend preview contract

Add a guarded preview command that accepts repository and registered worktree
identity, then returns:

- current actionability and refusal reason;
- clean/dirty state;
- active/busy lease state;
- branch-retained guarantee;
- total worktree bytes when measurement completes;
- bytes per recognized cache category;
- protected/unknown ignored-local-state presence;
- lifecycle identity and last-used time.

The preview is advisory. Mutation repeats every authorization check under the
exclusive lease.

## Cache categories

Start with a deliberately small allowlist, derived by the Rust backend from the
validated worktree root. Candidate categories must be verified against project
manifests before landing, with likely first entries:

- root Cargo `target/`;
- Desktop Tauri `desktop/src-tauri/target/`;
- Desktop build output `desktop/dist/`;
- root `node_modules/` when it is a real directory inside the worktree;
- mobile generated output only after confirming deletion does not violate the
  repository's Flutter safety rules;
- Hermit checkout-local state only after proving it is fully reconstructible and
  not shared through symlinks.

Do not delete `.env`, arbitrary ignored paths, package-manager stores outside the
worktree, Git metadata, or anything reached through a symlink.

The mutation accepts category IDs, not paths. For every category:

1. compute the canonical expected child from the validated worktree root;
2. use lstat/symlink checks and parent containment;
3. acquire the exclusive lease;
4. revalidate;
5. remove only the selected category;
6. return per-category bytes/result.

Cache clearing may be allowed on a dirty checkout because it does not touch
source state, but it still refuses while an active lease is held.

## Eviction contract

Replace user-facing `Remove worktree` language with **Free local space** or
**Evict checkout**. Keep `remove_project_worktree` temporarily as a compatibility
shim over the new guarded eviction implementation.

Eviction requires:

- verified managed path and branch;
- verified same-channel lifecycle identity;
- exclusive lease;
- clean tracked/untracked Git status;
- explicit confirmation if protected/unknown ignored local state exists;
- no `--force`;
- branch and durable record retained;
- eviction generation advanced only after successful remove/prune.

Return typed per-path results for bulk actions instead of collapsing all errors
into a count.

## Frontend UX

Update the channel Worktrees drawer:

- show reclaimable cache bytes separately from total checkout bytes;
- default the primary action to clear generated cache when available;
- show eviction as a stronger secondary action;
- state `Branch kept; the checkout will be recreated on the next agent turn`;
- name protected ignored/local-state risk before confirmation;
- make other-channel, legacy, conflict, active, and external entries read-only;
- measure expanded rows on demand with bounded concurrency (maximum chosen by a
  tested scheduler, not an unbounded loop);
- show per-path success/refusal after bulk operations.

No new module-level cache may escape `resetCommunityState()`.

## Tests

Rust (verified):

- [x] each allowed category is deleted and unrelated siblings remain;
- [x] arbitrary path/category, traversal, symlink (leaf + intermediate), external
  worktree, main worktree, and active lease are refused;
- [x] dirty source does not block cache-only reclaim but blocks eviction;
- [x] other-channel / legacy / missing-root forged calls refuse cache clear and
  eviction;
- [x] successful eviction retains branch/record and advances generation;
- [x] generation advance failure is not swallowed after successful remove;
- [x] preview separates `canClearCache` vs `canEvict` when dirty.

Frontend (verified):

- [x] cache-first action ordering and byte labels;
- [x] branch-retention and ignored-state confirmation copy;
- [x] read-only states never enter selection;
- [x] bounded detail loading (no prefetch-all);
- [x] callers pass routing channel ID into reclaim IPC.

E2E/manual against real manager worktrees: **not run** (fixtures only per safety
rules). Disposable fixture coverage lives in Desktop/ACP unit tests above.

## Exit gate

- [x] A manager can recover generated disk without deleting the checkout
  (cache-clear path).
- [x] Eviction is accurately described, branch-preserving, session-safe, and
  authorized in Rust.
- [x] Final focused suites, `pnpm run check`, and Rust fmt/clippy pass on the
  original base (re-run after main integration).

## Rollback

Hide the new controls and leave the compatibility command guarded. Cache paths,
branches, and durable records are not migrated or renamed by this phase.
