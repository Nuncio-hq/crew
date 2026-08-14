# Spike 0043 — Work-tree eligibility selector (#203)

- **Status:** PASS
- **Date:** 2026-08-14
- **Issue:** [#203](https://github.com/Nuncio-hq/crew/issues/203)

## Question

Can a pure selector over existing stores decide tree membership
correctly (workspace ∨ recent session ∨ needs-you) and stay cheap over
hundreds of threads in one channel?

## Decision affected

D-062: tree shows work, timeline shows talk. No new authoritative
state.

## Hypothesis

Union of worktree registry, observer workspace snapshots, active/recent
sessions, and needs-you is enough. Talk-only threads never enter the
map. 400 eligible rows collect in well under 50ms.

## Scope

- `desktop/src/features/work-tree/lib/workTreeEligibility.ts`
- `desktop/src/features/work-tree/lib/workTreeCollect.ts`
- Unit tests in `*.test.mjs`

## Exclusions

Live relay. Heuristic promotion by message volume. Mobile sidebar.

## Pass criteria

Eligibility true only for the three arms. Stale session without
workspace/needs-you drops. 400-row collect < 50ms.

## Fail criteria

Talk threads appear. Selector allocates a store. Collect is
multi-hundred-ms over hundreds of rows.

## Environment

Node unit tests via `desktop/test-loader.mjs`. No Postgres/Redis.

## Method

Table-driven eligibility + collect fixtures, including a 400-workspace
timing assertion.

## Results

`isWorkThreadEligible` is true for each arm and false for talk-only.
`collectWorkThreads` drops a 49h-old session and keeps needs-you /
workspace. 400-row collect stays under the 50ms bound.

## Edge cases observed

A workspace snapshot with no `channelId` still joins when needs-you
already supplied the root's channel.

Shared access channels (many repos on `#general`) are not folders.

## Limitations

Timing bound is a laptop/CI smoke check, not a profiler.

## Verdict

PASS — selector is sufficient; no new store.

## Follow-up test contract

Unit: eligibility, stale-session drop, large-N collect. E2E: exclusive
30617 on `#engineering` without folderizing `#general`.

## Cleanup

No disposable spike code remains; production lives under
`desktop/src/features/work-tree/`.
