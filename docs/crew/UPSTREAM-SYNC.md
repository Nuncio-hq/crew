# Upstream Sync Runbook

## Repository topology

```text
upstream  https://github.com/block/buzz.git      fetch only
origin    https://github.com/Nuncio-hq/crew.git  fetch and push
```

Crew is a GitHub fork, not an unrelated copy. Keep GitHub's fork relationship
and the local `upstream` remote.

The machine-readable baseline is
[`upstream-buzz.json`](upstream-buzz.json). Updating it is an explicit sync
decision, not a side effect of building or releasing NuncioCrew.

The local upstream push URL is deliberately disabled. Never push to
`block/buzz`.

## Thin-fork rules

- Prefer Crew-owned files under a new namespace.
- Do not restyle or reorganize upstream code.
- Do not copy upstream modules into parallel Crew versions.
- Existing upstream-file edits require explicit justification.
- The normal UI integration budget is one route-registration edit and one
  navigation-entry edit.
- A Rust edit requires a failed or insufficient non-Rust spike plus explicit
  approval.
- Keep `main` green.
- Use short-lived area branches; do not create long-lived component branches.
- Perform upstream integration on `sync/upstream-YYYY-MM-DD`.

## Before feature work

```bash
git status --short --branch
git fetch --prune origin
git fetch --prune upstream
git switch main
git pull --ff-only origin main
```

If `main` is not clean, stop. Do not hide or discard local changes.

## Sync procedure

```bash
git fetch --prune upstream
git fetch --prune origin
git switch main
git pull --ff-only origin main
git switch -c sync/upstream-YYYY-MM-DD
git merge --no-edit upstream/main
```

Then:

1. Record the chosen Buzz version, tag, and exact merged commit in
   `upstream-buzz.json`.
2. Inspect every conflict as a fork-maintenance signal.
3. Prefer moving Crew behavior into additive files over repeatedly resolving
   the same upstream file. If the Desktop file-size ratchet trips on a shared
   file, extract Crew-owned additions into Crew-only files so the shared file
   returns to at or below the upstream line count (D-022) — do not raise the
   limit or grant a sync-only exception. For a file with a recorded baseline,
   its recorded `lines` value uses `wc -l` semantics: update only that value to
   the exact count printed by the guard in the same sync PR when upstream grows
   it; Crew-owned growth still requires extraction under D-022.
4. After the merge, run `gh workflow list --all` and disable any newly imported
   workflows that fall outside Crew scope (for example Sprig publication).
5. Run focused tests for conflict areas.
6. Run upstream's required quality gates.
7. Review the fork delta:

```bash
git diff --stat upstream/main...HEAD
git diff --name-status upstream/main...HEAD
```

1. Push the sync branch and merge it through a reviewed PR into Crew `main`.

Run the manual `NuncioCrew Upstream Sync` workflow on the sync branch before
merge. Normal feature PRs intentionally do not run the inherited multi-product
Buzz matrix; see [`CI.md`](CI.md).

In Actions, select the sync branch instead of accepting the default `main`
branch. The CLI equivalent is:

```bash
gh workflow run nuncio-crew-upstream-sync.yml \
  --ref sync/upstream-YYYY-MM-DD
```

Confirm the resulting run's head SHA equals the sync branch HEAD before using
it as compatibility evidence.

## Feature branches

Use short-lived, area-prefixed names, for example:

```text
docs/crew-foundation
project/local-workspace-location
board/event-projection
agents/project-context
sync/upstream-2026-07-30
```

Branch names describe durable product areas, not plan phase numbers or audit
codes.

## Conflict policy

When upstream changes one of Crew's few edited files:

1. Re-read the upstream intent.
2. Reapply only the smallest Crew hook.
3. Move new logic into Crew-owned files.
4. Run the relevant upstream test and Crew contract test.
5. Record a decision if the maintenance boundary changes.

Do not resolve conflicts by keeping "ours" wholesale.

## Fork-drift review

After every upstream sync, classify the delta:

| Class                                 | Expected action                            |
| ------------------------------------- | ------------------------------------------ |
| Added Crew file                       | Normal                                     |
| Route/nav hook                        | Keep tiny and inspect manually             |
| Upstream file with repeated conflicts | Redesign integration boundary              |
| Rust modification                     | Revalidate the spike evidence and approval |
| Copied upstream implementation        | Remove or replace with composition         |

The goal is not a zero diff. The goal is a small, understandable, replayable
diff whose maintenance cost is visible.

## Recovery

If a sync branch becomes confused, do not rewrite or reset a dirty user
checkout. Preserve the branch, create a fresh sync branch from clean `main`,
and compare the attempts. Destructive Git recovery requires explicit approval.
