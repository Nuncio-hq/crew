# Phase 1 — Merge the release tag and clear mechanical conflicts

Status: **Ready** · Depends on: Phase 0 (Phase 3 of `260804-2040-gh-path-pr-visibility` merged)

## Problem

Crew is pinned at `desktop-v0.5.3` (`3a96acea0`). Upstream published
`desktop-v0.5.5` (`8342dfcc5`) — 86 commits, including two RUSTSEC batches. A
straight merge produces 30 conflicted files.

## Approach

Merge the **release tag**, not `upstream/main`. They are the same commit today
(`desktop-v0.5.5..upstream/main` is empty) but main drifts and the tag does not.

Resolution rule for every conflict: **take upstream's version, then re-apply the
Crew delta on top.** Do not hand-merge line by line — upstream rewrote large
regions and a line-level merge silently reverts their intent.

Five files carry real Crew product logic and are *not* forced closed here; they
are resolved to upstream and carried into Phase 2.

## Files

```bash
git switch main && git pull --ff-only origin main
git switch -c sync/upstream-2026-08-05
git merge --no-edit desktop-v0.5.5
```

Conflicts, 30 files:

| Cluster | Files |
|---|---|
| `desktop/src/features/messages/ui` (7) | `ComposerReplyEditBanner.tsx`, `MessageComposer.tsx`, `MessageComposer.types.ts`, `MessageThreadPanel.tsx`, `MessageTimeline.tsx`, `TimelineMessageList.tsx`, `useMentionSendFlow.ts` |
| `desktop/src/features/projects` (6) | `hooks.ts`, `repoSyncHooks.ts`, `useProjectsRepoSnapshots.ts`, `lib/projectLocalRepos.ts`, `ui/ProjectDetailScreen.tsx`, `ui/ProjectsView.tsx` |
| `desktop/src/features/channels` (4) | `ui/ChannelPane.tsx`, `ui/ChannelScreen.tsx`, `ui/ChannelScreenHeader.tsx`, `useChannelPaneHandlers.ts` |
| `mobile` (5) | `lib/features/channels/{channel_detail_page,thread_detail_page}.dart`, `lib/shared/relay/{relay_session,relay_socket}.dart`, `test/shared/relay/relay_session_test.dart` |
| `desktop/src-tauri/src` (4) | `lib.rs`, `initial_window.rs`, `commands/agent_discovery.rs`, `commands/agent_discovery/managed_node.rs` |
| crates (3) | `buzz-acp/src/lib.rs`, `buzz-acp/src/pool.rs`, `buzz-sdk/src/builders.rs` |
| `desktop/src/features/communities` (1) | `useCommunityInit.ts` |

**Carried to Phase 2** (resolve to upstream here, re-home there): `hooks.ts`,
`repoSyncHooks.ts`, `useProjectsRepoSnapshots.ts`, `lib/projectLocalRepos.ts`,
`ui/ProjectsView.tsx`.

## Steps

1. `. ./bin/activate-hermit` before any git or hook operation.
2. Create the branch and merge the tag as above.
3. Resolve non-`features/projects` conflicts with the take-upstream rule.
   `useCommunityInit.ts` needs care: Crew's `resetCommunityState()` singleton
   list must survive, and any new upstream singleton must be added to it
   (AGENTS.md § Community Switching).
4. `crates/buzz-acp/src/lib.rs` — Crew's added `mod` declarations
   (`conversation`, `elicitation`, `retry_turn`, `thread_workspace`,
   `thread_workspace_tests`) must survive. Check upstream #4395
   (`_meta.systemPrompt`) against Crew's ACP modules for behavioral overlap.
5. `git commit -s` (DCO gate). The pre-commit hook runs `just desktop-tauri-fmt`,
   which fails inside a worktree — run that recipe from the main checkout, then
   re-stage.

## Validation

- `cargo check --workspace`
- `cargo check --manifest-path desktop/src-tauri/Cargo.toml --workspace --all-targets`
- `cd desktop && pnpm lint && pnpm test`
- `git merge-base --is-ancestor desktop-v0.5.5 HEAD` returns 0

## Risk

Taking upstream wholesale can silently drop a Crew behavior that had no test.
Mitigation: Phase 2's acceptance test exercises the local-path and worktree
surfaces end to end.

## Rollback

Delete the branch. Nothing is pushed to `main` until the gate is green.
