# Final Scoped Code Review — Thread Worktree Blockers

Date: 2026-07-31
Worktree: `/Users/a1241968/Desktop/Oscar/LilGroup/Nuncio/.worktrees/crew-thread-worktree-ui`
Scope: only the seven previously confirmed blockers, on current uncommitted bytes

## Verdict

No remaining actionable finding in the seven-item scope. All seven blockers are
resolved.

## Resolution Evidence

1. **Exact root required — resolved.**
   `parse_nostr_thread_response` now constructs the thread only from
   `root_msg?`, and places that exact event first
   (`crates/buzz-acp/src/pool.rs:3359-3392`). A reply-only response returns
   `None`, verified by
   `pool::tests::thread_response_requires_the_exact_requested_root`
   (`crates/buzz-acp/src/pool.rs:4162-4176`).

2. **Typed workspace errors — resolved.**
   Trusted-root selection, workspace parsing, and worktree creation all map
   through `ThreadWorkspaceProvisionError`
   (`crates/buzz-acp/src/pool.rs:2794-2818`). The outer prompt path downcasts
   that type and emits the path-free `thread_workspace_error` payload
   (`crates/buzz-acp/src/pool.rs:1464-1478`). Missing-root fail-closed behavior
   and safe payload content are covered at
   `crates/buzz-acp/src/pool.rs:2882-2886` and
   `crates/buzz-acp/src/pool.rs:2962-2988`.

3. **Community restore — resolved.**
   The outgoing community projection is saved before singleton reset
   (`desktop/src/features/communities/useCommunityInit.ts:181-198`) and restored
   after the destination community applies, before ready render
   (`desktop/src/features/communities/useCommunityInit.ts:263-274`). Snapshots
   remain isolated by community and are consumed on restore
   (`desktop/src/features/agents/projectThreadWorkspaceStore.ts:157-185`).
   The A to B to A regression is covered by
   `desktop/src/features/agents/projectThreadWorkspaceStore.test.mjs:172-206`.

4. **Legacy atomic winner and reuse — resolved.**
   The durable claim lives under the repository common Git directory and uses
   atomic `create_new`; a colliding root can only read/validate the already
   created claim (`crates/buzz-acp/src/thread_workspace.rs:277-367`). Branch
   metadata is recorded only after the root claim succeeds, then revalidated
   (`crates/buzz-acp/src/thread_workspace.rs:298-305`). The collision regression
   requires exactly one winner, successful winner reuse, and winner-only
   durable config
   (`crates/buzz-acp/src/thread_workspace_tests.rs:142-198`).

5. **Observer ordering — resolved.**
   Each root retains a timestamp, sequence, and agent watermark; older or equal
   non-winning frames are rejected before projection mutation
   (`desktop/src/features/agents/projectThreadWorkspaceStore.ts:31-71`).
   Both stale-error-after-ready and stale-ready-after-error cases are covered
   (`desktop/src/features/agents/projectThreadWorkspaceStore.test.mjs:106-148`).

6. **Projection cap — resolved.**
   The active projection has a documented 256-root LRU cap, refreshes recency
   on read, and evicts the least-recently-used root on overflow
   (`desktop/src/features/agents/projectThreadWorkspaceStore.ts:25-29`,
   `desktop/src/features/agents/projectThreadWorkspaceStore.ts:68-83`,
   `desktop/src/features/agents/projectThreadWorkspaceStore.ts:146-154`).
   Cap eviction is covered at
   `desktop/src/features/agents/projectThreadWorkspaceStore.test.mjs:208-247`.

7. **Explicit error UI — resolved.**
   Subtitle, chip, and body now have explicit pending, ready, and error
   branches; only pending renders the spinner
   (`desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx:113-171`).
   The E2E regression asserts `Failed`, the safe error, and absence of both
   `Preparing` and shared-worktree copy
   (`desktop/tests/e2e/project-thread-worktree.spec.ts:226-252`).

## Targeted Verification

- `cargo test -p buzz-acp thread_workspace -- --nocapture` — 8 passed.
- `cargo test -p buzz-acp thread_response_requires_the_exact_requested_root` —
  1 passed.
- `cargo test -p buzz-acp missing_root_context_fails_closed` — 1 passed.
- `node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/projectThreadWorkspaceStore.test.mjs`
  — 8 passed.

## Unresolved Questions

None.

**Status:** DONE
**Summary:** All seven previously confirmed blockers are resolved in final source and targeted regressions.
**Concerns/Blockers:** None within the requested review scope.
