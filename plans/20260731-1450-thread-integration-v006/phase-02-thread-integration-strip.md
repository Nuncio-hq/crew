# Phase 02 — Thread integration strip (2×3) + handoff from replies

- **Status:** Complete
- **Priority:** high

## Context

Today the panel stacks three cards (`ProjectThreadWorkspacePanel.tsx`) costing
~232px of thread height, and its handoff list is built from
`initialAgentPubkeys` in `MessageThreadPanel.tsx:516-537`, which reads mentions
from the **thread root only**. An agent pulled in by a later reply runs in the
same worktree but never appears — observed live with Cursor Grok High Fast.

## Requirements

1. Two rows of three cells. Row 1: task · workspace · handoff. Row 2: issue ·
   PR · CI. Each cell opens one detail drawer below the strip; one open at a
   time; Escape closes.
2. Row 2 renders only when the thread branch has a pull request. With no PR,
   keep the GitHub row hidden.
3. Handoff steps come from root mentions **and** reply mentions, in first-seen
   order, root first. Each row shows how the agent entered the thread.
4. Lifecycle actions live in the drawers: `Close PR` (PR drawer),
   `Delete branch` and `Remove worktree` (workspace drawer). Each confirms
   first. `Remove worktree` is disabled with a reason while the worktree is
   dirty.
5. Workspace drawer shows the base revision and, when Phase 01 reports it,
   `N behind origin/main`.

## Colors — hard constraint

Use only tokens already in `desktop/src/shared/styles/globals/theme.css` and
stock Tailwind classes. Statuses reuse the existing
`text-emerald-600 dark:text-emerald-400` treatment from the current panel and
`text-destructive` for failures. **Do not add CSS variables and do not
introduce arbitrary px or rem text sizes** — `pnpm check:px-text` fails the
build on new literals; meta text belongs on `text-2xs` / `text-3xs`.

## Files

- `desktop/src/features/messages/ui/ProjectThreadWorkspacePanel.tsx` (rewrite layout)
- `desktop/src/features/messages/ui/ProjectThreadWorkspaceDetails.tsx`
- new drawer components under `desktop/src/features/messages/ui/`
- `desktop/src/features/messages/lib/projectThreadWorkspace.ts` (`buildProjectThreadAgentSteps`)
- `desktop/src/features/messages/ui/MessageThreadPanel.tsx` (`initialAgentPubkeys`)
- new GitHub read layer + Tauri command wrapping `gh`
- `desktop/src/testing/e2eBridge.ts`, `desktop/tests/e2e/project-thread-worktree.spec.ts`

## Steps

1. Extend the agent-step derivation to accept reply mentions. Keep it a pure
   function with its own `.test.mjs` — root-only, reply-only, duplicate across
   both, and ordering cases.
2. `initialAgentPubkeys` must merge root mentions with mentions found on thread
   replies, preserving root priority and first-seen order after that.
3. Rebuild the panel as a 3-column grid row plus an optional second row, with a
   single shared drawer region. Keep each new file under ~200 lines.
4. GitHub data: one Tauri command shelling out to `gh` with `--json`, scoped to
   the thread branch. Resolve the PR by head branch, the issue from the PR's
   linked issues or closing keywords, and checks from the PR's status rollup.
   `gh` missing or unauthenticated → return "unavailable" and hide row 2; never
   surface a raw CLI error in the thread.
5. Cache per branch with a short TTL and refresh on drawer open, so the strip
   does not spawn a `gh` process per render.
6. If a new module-level cache is introduced, register its reset in
   `resetCommunityState()` (`useCommunityInit.ts`) — community switching must
   not leak the previous relay's PR data.
7. Extend the e2e spec: reply-added agent appears; row 2 hidden without a PR;
   `Remove worktree` disabled while dirty. Build with `pnpm build:e2e`.

## Validation

```bash
cd desktop && pnpm test && pnpm check:px-text
cd desktop && pnpm build:e2e
cd desktop && pnpm exec playwright test tests/e2e/project-thread-worktree.spec.ts --project=smoke
just ci
```

Phase-local results: 3,887 desktop tests passed with one skipped; typecheck and
`check:px-text` passed; the focused Project thread E2E passed 2/2. The full
Tauri, desktop smoke, and `just ci` gates remain in the pre-review release gate.

Screenshots for the PR via `just desktop-screenshot`, posted with
`scripts/post-screenshots.sh` — never relay media URLs. Verify hashes are
distinct before posting.

## Risk

`gh` latency on the render path. Keep every GitHub read off the initial paint:
the strip renders from cache or an unavailable state first, then fills in.

## Rollback

Revert the commit; the panel returns to the stacked-card layout. No persisted
state changes.
