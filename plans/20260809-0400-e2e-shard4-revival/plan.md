# Plan — #109 Revive e2e shard 4

Spec: [#109](https://github.com/Nuncio-hq/crew/issues/109)
Status: not started. **Highest priority of the six open issues.**

## Outcome

`Desktop Smoke E2E (4)` completes within its job timeout on `main` and reports a
real pass/fail verdict for all 250 of its tests.

## Why this is first

The lane has been `cancelled` at 30 minutes on **6 of 6 consecutive `main` runs**
since `b57d26def` (#95). At `e41a1a6a4` it was killed after **test 106 of 250** —
**144 tests never executed at all**.

Every other open issue (#102, #104, #105, #111) ships UI into the surfaces this
lane covers. Advisory or not, this lane is the only one that has ever caught a
*code-alive-but-unwired* regression in this fork. Building four features on top
of a dead net is the expensive order.

## Root cause shape (hypothesis, not yet proven)

19 upstream `project-*` tests each fail on a **uniform 31.4–31.6s** locator
timeout, ×3 attempts ≈ 95s each. That arithmetic alone exceeds the job budget.

Four independent spec files failing at an identical duration on their first
assertion points at **one shared entry point** — a testid, route, or project-view
surface that #95 renamed, moved, or removed — not 19 independent breaks.

All timing-out specs are **upstream-owned** (present at `desktop-v0.5.7`).
`project-outcomes.spec.ts`, the only Crew-owned project spec, is **not** failing.
That asymmetry is itself evidence: Crew's own spec was updated with #95, upstream's
were not.

## Phases

### Phase 0 — Prove the shared cause (do not skip)

- [ ] Sibling worktree at `origin/main`; `pnpm build:e2e`.
- [ ] Run `project-inbox.spec.ts:9` alone — smallest failing spec (146 lines).
- [ ] Capture the exact locator that times out.
- [ ] Diff #95's changes against the component that locator targets.
- [ ] Confirm the same locator is the first failing assertion in
      `project-commit-detail`, `project-issue-comments`, and `project-pr-review`.

**Gate:** either one shared cause is named with evidence, or the work is
re-planned as N independent breaks. Do not start fixing before this.

### Phase 1 — Decision fork (needs Oscar)

Two branches, materially different cost:

- **#95 broke the surface accidentally** → fix product code. One fix likely
  revives the whole lane. Proceed without asking.
- **#95 intentionally redesigned the surface** → choose:
  - adapt ~19 upstream spec files → permanent fork delta, conflicts on **every**
    upstream sync; or
  - skip them + file a Crew-native replacement issue → the #65 precedent, honest
    about the coverage loss on the surface Crew changed most.

This is a product call. Post the evidence and a recommendation; do not pick
silently.

### Phase 2 — Land and verify

- [ ] Fix or skip per Phase 1.
- [ ] Shard 4 **completes** (not `cancelled`) at the PR head.
- [ ] Freeze the branch tip long enough for one full 4-shard run — pushes cancel
      in-flight runs, which is how this lane reached merge with zero completions
      during the v0.5.5 sync.

### Phase 3 — Stop it going dark silently

- [ ] The two fast failures in shard 4 (`overscroll-boundary.spec.ts:34`,
      `profile-active-turn.spec.ts:107`) are unrelated to the timeout — triage
      separately, do not bundle.
- [ ] Decide whether `Desktop Smoke E2E` should join `gate.needs`. It caught the
      v0.5.5 sync's only real regression while no gate job did. Separate PR:
      this touches CI config.

## Acceptance criteria

- Shard 4 conclusion is `success` or `failure` — never `cancelled` — on `main`.
- A verdict exists for all 250 shard-4 tests.
- **Raising the job timeout is not an acceptable fix on its own.** The timeout is
  a symptom of retry arithmetic; a longer budget still leaves ~19 red tests with
  zero coverage and hides the regression for longer.

## Risks

- Shard composition shifts whenever spec files are added, so "shard 4" is not a
  stable set. Attribute by **test name** from the per-shard report, never by
  shard colour.
- `project-pr-review.spec.ts` is 1383 lines of upstream spec. Adapting it is a
  large permanent fork delta — cost it honestly before choosing that branch.

## Evidence

Runs `31263949909`, `31256878409`, `31253637576`, `31188797720`, `31170460252`,
`31156789903`, `31153733457`, `31143687790`, `31068758927`.
Last all-green e2e on `main`: `9f5fe05a1` (2026-08-06 03:33Z).
