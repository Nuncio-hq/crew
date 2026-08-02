# Open issues — sticky project-thread bar: hit target + drawer coverage

**Status:** planned, not started
**Issues covered:** [#31](https://github.com/Nuncio-hq/crew/issues/31),
[#32](https://github.com/Nuncio-hq/crew/issues/32) — the full open set on
`Nuncio-hq/crew` as of 2026-08-02 13:42Z
**Base:** `main` @ `5c79c8cc2` (merge of PR #27), CI green on that SHA
**Scouted from:** worktree `crew-1ade767569df` on `buzz/1ade767569df`

## Outcome

The sticky project-thread status bar is fully clickable in docked mode for
humans, and `project-thread-worktree.spec.ts` asserts the sticky bar's real
behaviour (drawer contents, error truth) instead of only chip presence.

## Scope change vs. the issues as filed

Scouting turned up a **third problem the issues do not name**, and it is the
actual blocker for #32's "PR drawer wedges page JS" note:

`ProjectThreadWorkspacePanel.tsx:109-117` refreshes GitHub in an effect keyed on
`[activeDrawer, model]`, but `useProjectThreadWorkspaceModel` returns a fresh
object literal every render (`useProjectThreadWorkspaceModel.ts:106-117`). While
the PR / CI / Issue drawer is open, every render forces a reload, every reload
replaces the store snapshot and notifies, every notify re-renders — a
self-feeding loop of `get_thread_github_status` invokes. That is a live
user-facing bug (pegged CPU, `gh` hammered), not e2e debt.

So the work is three phases, not two.

## Phases

| # | Phase | Issue | Blocks |
|---|---|---|---|
| 1 | [Docked title hit target](phase-01-docked-title-hit-target.md) | #31 | — |
| 2 | [GitHub drawer refresh loop](phase-02-github-drawer-refresh-loop.md) | new | phase 3 |
| 3 | [Restore sticky-bar e2e coverage](phase-03-restore-sticky-bar-e2e.md) | #32 | needs 1 + 2 |

Phases 1 and 2 are independent and can run in parallel (different files).
Phase 3 needs both: it re-asserts drawer contents (phase 2) through chips that
must be clickable (phase 1).

## Open question for the founder

Phase 1's first step is a repro that may close #31 without a code change — see
that phase. Nothing else in the plan depends on the answer; phases 2 and 3 stand
either way.

## Acceptance for the whole plan

- [ ] #31 closed: either fixed with a regression test, or closed as
      already-fixed-by-#27 with the evidence recorded on the issue
- [ ] PR/CI/Issue drawer opens without a refresh loop, proven by an invoke-count
      assertion — not by "it felt fast"
- [ ] `project-thread-worktree.spec.ts` asserts drawer contents and the workspace
      error path again, with no force-clicks or locator hacks
- [ ] `just ci` green; `pnpm test:e2e:smoke` green from a clean `pnpm build:e2e`
