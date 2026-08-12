# Session-safe channel worktree reclaim — plan index

- **Status:** Implemented / verified on current `origin/main`
- **Date:** 2026-08-05
- **Owner:** Oscar
- **Issue:** [Nuncio-hq/crew#59](https://github.com/Nuncio-hq/crew/issues/59)
- **Branch:** `plan/issue-59-safe-channel-worktree-reclaim`
- **Builds on:** [`plans/260802-1457-channel-worktree-management/`](../260802-1457-channel-worktree-management/README.md)

## Outcome

Let a manager reclaim Project-thread disk space from a channel without deleting
work, racing an agent, or touching another channel's or a human's checkout.
Cleanup has two distinct meanings:

1. **Clear generated cache** — remove only backend-owned, allowlisted build/tool
   directories. Source state and the checkout remain.
2. **Free local space / evict checkout** — remove a clean managed checkout while
   retaining its deterministic branch and root claim. A later turn reattaches it.

Deleting a branch remains a separate explicit action and is never part of
reclaim or an automatic policy.

## Implementation verification (2026-08-05)

Phases 1–4 are implemented and verified on a branch rebased onto current
`origin/main` (integration pass; commit SHA intentionally not pinned here).
Focused gates re-run after semantic conflict resolution:

| Gate | Result |
|------|--------|
| `cargo test -p buzz-worktree` (unit + cross-process lock tests) | PASS |
| `rustup run 1.88.0 cargo test -p buzz-worktree --lib` (MSRV) | PASS |
| `cargo test -p buzz-acp resolve_and_bind` (lease-before-ensure, race, self-heal) | PASS (8/8) |
| `cargo test -p buzz-acp thread_workspace` (incl. detached recovery) | PASS (23) |
| Desktop Tauri `project_worktree_*` tests | PASS (40) |
| Desktop Tauri `thread_workspace_*` tests | PASS (6) |
| Frontend `worktreeBuckets.test.mjs` + destructive-action tests | PASS (17+4) |
| `pnpm run check` | PASS (warnings only, pre-existing) |
| `cargo fmt --all -- --check` + desktop Tauri fmt check | PASS |
| `cargo clippy -p buzz-worktree -p buzz-acp --all-targets -- -D warnings` | PASS |
| `cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --lib -- -D warnings` | PASS |

## Integration status

**Integrated onto current `origin/main`.** Semantic conflict resolution preserved
upstream multi-repo detached recovery (`automatic_recovery_is_safe` + lossless
check), linked-issue registry/UI rollup, and desktop terminal deps, while keeping
issue #59 safety invariants (lease-before-ensure, exclusive mutation auth,
fail-closed drawer buckets, cache allowlist/symlink guards, branch-retaining
eviction). **P3 / Phase 5 completed by [#174](https://github.com/Nuncio-hq/crew/issues/174)**
as suggest-and-confirm + observed-time idle (not background auto-quota).

## Verified baseline (pre-implementation)

The pre-change implementation already provided repo-scoped discovery and guarded
manual removal. Investigation noted unsafe drawer claims (`other-channel` from
timeline absence), cached ACP sessions that skipped cwd revalidation, and
prefetch-all details. Focused baseline gates at plan time: 9 Rust cleanup tests
and 7 frontend bucket tests. A local audit measured ~115 GiB across sibling
worktrees, mostly generated/tool directories — motivating cache-first reclaim.

## Locked decisions

### D1 — The lifecycle unit is a thread; the management view is a channel

One Project channel can own many task-thread worktrees. Every durable record is
keyed by the full thread-root event ID and carries its real routing channel ID.
The channel drawer is only a filtered view over that repo-local registry.

### D2 — Destructive authorization remains in Rust

Frontend buckets, observer frames, and active-turn state are presentation
signals, not authorization. Every cache clear or eviction revalidates the repo,
registered worktree, canonical managed root, managed branch, lease, lifecycle
record, routing channel, and target immediately before mutation.

### D3 — Active turns hold a shared lease; eviction needs an exclusive lease

The ACP harness acquires a shared cross-process lease **before** any worktree
ensure/create/reattach, and holds it through the turn. Cleanup uses a
non-blocking exclusive lease and returns a typed `refused` result if any active
turn owns the workspace. Cached sessions revalidate under a shared lease;
eviction generation and checkout recreate/reattach force session invalidation.

### D4 — Shared lifecycle mechanics live in an additive internal crate

`crates/buzz-worktree` is consumed by both `buzz-acp` and Desktop Tauri. Leases
use `fs4` (MSRV-compatible; no std File locks that require Rust 1.89+). No
home-grown PID lockfile protocol and no `unsafe` in the crate.

### D5 — Durable metadata is local and stored under the common Git directory

Versioned records live below the repo's common Git directory alongside the
existing `buzz-thread-workspace-roots` claims. They are never committed or
published to the relay. Record identity fails closed when path/body/requested
root disagree.

### D6 — Legacy entries fail closed

Existing worktrees without the new record remain visible as
`Legacy / channel unknown` and are not channel-actionable for reclaim until
adopted by a trusted ACP turn. Rust refuses legacy/no-root/conflict/other-channel
mutations.

### D7 — Cache reclaim is allowlist-based, not ignore-based

The backend computes known generated paths from a validated worktree root.
Frontend sends category IDs only. Any symlink component from worktree root to
the allowlisted target is refused.

### D8 — Branches are retained by every reclaim action

Eviction runs `git worktree remove` and `git worktree prune`, then advances
eviction generation (never swallowed on success). It does not run
`git branch -D` or remote deletion.

## Delivery phases

| Phase | Status | Delivers | Depends on | Plan |
|---|---|---|---|---|
| 1 | **Implemented / verified** | Fail-closed channel scoping and truthful current UX | — | [phase-01-fail-closed-scope.md](phase-01-fail-closed-scope.md) |
| 2 | **Implemented / verified** | Cross-process active lease and cached-session self-heal | 1 | [phase-02-lease-and-session-revalidation.md](phase-02-lease-and-session-revalidation.md) |
| 3 | **Implemented / verified** | Durable channel identity and real `lastUsedAt` | 2 | [phase-03-durable-lifecycle-metadata.md](phase-03-durable-lifecycle-metadata.md) |
| 4 | **Implemented / verified** | Cache-first reclaim and eviction UX | 3 | [phase-04-cache-reclaim-and-eviction.md](phase-04-cache-reclaim-and-eviction.md) |
| 5 | **Deferred / follow-up** | Opt-in quota/LRU policy gate | 4 + production evidence | [phase-05-optional-policy.md](phase-05-optional-policy.md) |

Phase 5 remains explicitly deferred. Do not implement it in this issue unless a
separate follow-up is opened after production evidence from manual reclaim.

## Whole-issue acceptance

- Cleanup/start races cannot remove a cwd while an agent is using it or let a
  cached session prompt against a removed cwd. **(verified on current main)**
- The channel drawer never authorizes a worktree whose channel identity is
  unknown or belongs to another channel; Rust enforces the same. **(verified)**
- Idle state comes from durable ACP usage time, not Git commit age. **(verified)**
- Cache clearing touches only validated allowlisted generated directories. **(verified)**
- Eviction preserves the branch and a later thread turn reattaches under lease.
  **(verified in unit/race fixtures)**
- Ignored/local checkout state refuses eviction while allowing cache clear;
  plain-clean porcelain alone never authorizes removal. **(verified)**
- Human/external worktrees remain read-only. **(verified)**
- No reclaim path uses `--force` or deletes a branch. **(verified)**
- Focused suites and fmt/clippy/`pnpm run check` pass on current main.
  **(verified after rebase + semantic conflict resolution)**

## Verification strategy

```text
. ./bin/activate-hermit
cargo test -p buzz-worktree
cargo test -p buzz-acp resolve_and_bind
cargo test -p buzz-acp thread_workspace
cargo test --manifest-path desktop/src-tauri/Cargo.toml project_worktree_
cargo test --manifest-path desktop/src-tauri/Cargo.toml thread_workspace_
cd desktop
node --import ./test-loader.mjs --experimental-strip-types \
  --test src/features/channels/lib/worktreeBuckets.test.mjs \
  src/features/messages/ui/projectThreadDestructiveActions.test.mjs
pnpm run check
cd ..
cargo fmt --all -- --check
cargo clippy -p buzz-worktree -p buzz-acp --all-targets -- -D warnings
just _ensure-sidecar-stubs
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --lib -- -D warnings
```

Use only disposable fixture worktrees for any manual reclaim — never real
manager checkouts.

## Safety and rollback

- Phase 1 is UI/classification-only and can be rolled back without changing disk
  metadata.
- The Phase 2 lease protocol is versioned; unknown versions fail closed.
- Phase 3 records are additive under the common Git directory.
- Phase 4 keeps `remove_project_worktree` as a compatibility shim over
  channel-authorized eviction.
- Unmounting the new controls stops mutations; branches and root claims remain.

## Explicit non-goals

- Publishing machine-local path, disk, or lease state to Nostr.
- Cleaning `.worktrees/` human checkouts.
- Automatic branch or remote deletion.
- A general-purpose filesystem cleaner.
- Sharing one Cargo target directory across concurrent branches in this issue.
- Implementing Phase 5 automatic quota/LRU in this delivery.
