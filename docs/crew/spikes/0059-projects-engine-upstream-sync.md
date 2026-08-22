# Spike 0059 — Projects engine upstream sync (#270)

- **Status:** PASS
- **Date:** 2026-08-22
- **Issue:** [#270](https://github.com/Nuncio-hq/crew/issues/270)
- **Related:** [#278](https://github.com/Nuncio-hq/crew/issues/278),
  [#285](https://github.com/Nuncio-hq/crew/issues/285),
  [block/buzz#5102](https://github.com/block/buzz/pull/5102),
  [block/buzz#5792](https://github.com/block/buzz/pull/5792)

## Question

Can Crew absorb the complete-repository-tree and Projects v3 engine changes
from Buzz 0.5.18 without restoring Buzz's Projects rail, top-level Projects
navigation, or Workbench-as-a-place?

## Decision affected

Whether #270 can port the two upstream engine slices independently of the
navigation and overview chrome rejected by D-065, D-066, and #278.

## Hypothesis

The repository reader, entity-link, discussion-discovery, and issue-assignment
contracts are separable from the upstream Projects navigation. Crew can port
those contracts into its existing project detail surfaces while retaining the
channel-first shell and fail-closed local-workspace rules.

## Scope and exclusions

Compared Crew `main`, currently pinned to Buzz `desktop-v0.5.11`
(`248b9d1b7666aacbcb1485b76e81de30a271ba0e`), with:

- `block/buzz#5102` final head `1ab881798`
- `block/buzz#5792` final head `ea9fc75b4`
- Buzz `desktop-v0.5.18` commit
  `39f8b46935736334cdd7045a4e4b5d7eb1a33888`

Excluded:

- Projects sidebar and overview rail
- top-level Projects navigation
- Workbench destinations or tabs
- unrelated Buzz 0.5.18 changes

## Pass/fail criteria

**PASS** if the engine slices can be manually ported into existing Crew
surfaces without changing `AppSidebar.tsx`, `AppSidebarPinnedHeader.tsx`,
`AppShell.tsx`, `useAppNavigation.ts`, or `routes.ts`.

**FAIL** if complete trees, canonical links, discussion lookup, or assignment
semantics require any of that navigation chrome.

## Environment

- Repository: `Nuncio-hq/crew`
- Branch base: Crew `main`
- Node 24.15.0 and pnpm 11.4.0 via Hermit
- Git comparison: three-way `git merge-tree` against each upstream PR base

## Method

1. Fetched Buzz `desktop-v0.5.18` and both upstream PR heads.
2. Compared each final PR head with its parent and with Crew `main`.
3. Ran three-way merge-tree analysis from each upstream PR base.
4. Classified conflicts as engine, Crew project-surface integration, or
   rejected navigation chrome.
5. Traced existing Crew repository readers, entity-link parsing, issue
   projection, relay/CLI builders, and channel-first routing.

## Results

### #5102 — manual engine port

The upstream change conflicts with Crew's evolved project detail and
exact-local-workspace code in `project_git.rs`, command registration,
`ProjectRepositoryPanel.tsx`, project helpers, shared project Git APIs, and E2E
bridge fixtures. The behavior is still separable:

- return all tracked metadata rather than taking the first 250 paths;
- cap eager content previews at 250 blobs;
- load deferred file content through the existing validated local/remote
  readers;
- paginate folder rows in the existing Crew repository panel.

Crew's canonical-path and exact-local-workspace checks remain authoritative.

### #5792 — selective engine port

The upstream branch also conflicts in `AppShell.tsx`,
`useAppNavigation.ts`, project routes, overview/detail chrome, and many
navigation-oriented Projects components. Those conflicts are not needed for
the engine contract.

The separable slice is:

- canonical `buzz://project`, repository, issue, pull-request, commit, and
  file links with parser round trips;
- channel discussion query/grouping helpers over existing message search;
- assignment/unassignment builders, CLI dispatch, relay-real fetches, causal
  projection, authorization, and partial-publish recovery;
- audit of first-clone-URL assumptions at project/repository call sites.

### Rejected upstream chrome

The following 0.5.18 changes are explicitly not ported:

- Projects rail or sidebar membership
- top-nav Projects entry restoration
- Projects overview navigation shell
- Workbench destination restoration

Crew continues to expose project details only through its channel-first
surfaces and existing routes.

## Edge cases observed

1. Crew's project surface has substantial post-0.5.11 exact-local-workspace
   behavior, so wholesale cherry-picks would delete or bypass fork-specific
   safety contracts.
2. Upstream assignment fixes after the initial #5792 commit are required:
   assignment fetches must be relay-real and independent of comment-window
   bounds, causal self-service operations need `prior` heads, and recovery
   must handle partially published multi-recipient writes.
3. Complete metadata must not imply complete eager content. The metadata list
   and content-preview budget are separate limits.

## Limitations

This spike establishes the integration boundary. The issue's RED/GREEN tests,
local gates, and mock-bridge E2E evidence determine whether the port is
correct.

## Verdict

**PASS.** Port #5102 and #5792 manually at their engine boundaries. Keep
Crew's existing sidebar, app shell, navigation hooks, routes, exact-local
workspace behavior, and Workbench removal unchanged.

## Follow-up test contract

- Rust parser retains paths after the eager-preview limit.
- Desktop pagination advances and clamps.
- Canonical project links round-trip through the entity parser.
- Assignment/unassignment tags, authorization, causal ordering, fetch bounds,
  and recovery behavior are covered at SDK/CLI/desktop boundaries.
- Existing channels-only sidebar E2E remains green.

## Cleanup

No temporary merge or cherry-pick was applied. Only fetched refs and Git
objects remain; no credentials or relay data were created.
