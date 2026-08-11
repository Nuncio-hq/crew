---
phase: 03
title: Gate / E2E-shard evidence audit (read-only)
status: pending
priority: high
effort: M
dependencies: []
---

# Phase 03 — Gate / E2E-shard evidence audit (read-only)

- **Issue:** #117 — problem item 3; DoD checkbox 4 (evidence half)
- **PR scope:** docs only — a verification record plus a `CI.md` correction.
  **No workflow edit in this phase.**
- **Files:** `docs/crew/verification/0007-gate-e2e-shard-relationship.md` (new),
  `docs/crew/CI.md`
- **Upstream files touched:** none. `.github/workflows/ci.yml` is inherited and is
  **read-only** here.

## Context and the already-answered question

The issue asks: "Either the Gate does not aggregate the smoke shards, or it
reported on a different run." **The first is true, and it is deliberate.** This
phase does not re-discover that; it writes it down with citations and supplies
the flake numbers Phase 04 needs.

Evidence, all in Crew-owned files:

| Fact | Citation |
| ---- | -------- |
| Smoke shards cannot fail the job | `.github/workflows/nuncio-crew-ci.yml:258` — `continue-on-error: true` |
| The intent is recorded in-line | `nuncio-crew-ci.yml:248-251` — "Advisory only until the smoke suite's ~5 moving failures per green run are attributed and quarantined (#37) … Do not add this job to `gate.needs` or JOB_RELEVANCE until it can fail for real reasons — see issue #36" |
| The gate does not depend on them | `nuncio-crew-ci.yml:320` — `needs: [changes, desktop-fast, desktop-rust, macos-arm, project-relay, buzz-acp]` |
| The pass/fail rule has no smoke entry | `desktop/scripts/check-nuncio-crew-ci-results.mjs:6-12` — `JOB_RELEVANCE` |
| The posture is contract-tested | `desktop/src/testing/nuncio-crew-ci-contract.test.mjs:130-156`, notably `:150` `assert.doesNotMatch(ci, /needs\.desktop-smoke-e2e\.result/)` and `:155` the same for the gate helper |
| Upstream Buzz does the opposite | `.github/workflows/ci.yml:295` `needs: [changes, desktop-core, desktop-smoke-e2e]`, `:306-307` fails on non-success |

So on PR #114, `NuncioCrew Gate` reporting SUCCESS while shards 1 and 3 were
FAILURE and shard 4 CANCELLED is **the designed behavior**, not a mis-reported
run. A red E2E state can and does merge to `main` — knowingly.

The real defect is a **documentation gap**: `docs/crew/CI.md:15-23` lists the six
gate jobs and never mentions `Desktop Smoke E2E` at all, so a reader concludes
the gate covers everything that runs. Nothing in `docs/crew/` tells an agent that
a green gate proves nothing about E2E.

## Flake-rate evidence to collect

Phase 04 cannot be decided without numbers. Gather from `main` runs of
`NuncioCrew CI` (issue #109 already contains the bisect table through
`e41a1a6a4` — extend it, do not redo it):

1. Per-shard conclusion for the last ~10 `main` runs (`success` / `failure` /
   `cancelled`).
2. For shard 4: confirm the 30-minute-timeout cancellations continue, and how many
   of the shard's 250 tests execute before the kill.
3. The currently-failing spec set per shard, and whether each is upstream-owned or
   Crew-only. #109 records that all the timing-out specs are upstream-owned at
   `desktop-v0.5.7` and that `project-outcomes.spec.ts` (the only Crew-only project
   spec) is not among them.
4. Whether #114's own triage attributed its shard 1/3 failures to flake or to a
   real regression — this is the input the issue defers to the #114 owner. If that
   triage has not landed, record the audit as complete-with-one-open-input rather
   than blocking; the workflow-config half needs nothing from #114.

```bash
gh run list --repo Nuncio-hq/crew --workflow "NuncioCrew CI" --branch main --limit 10 \
  --json databaseId,headSha,conclusion,createdAt
gh run view <id> --repo Nuncio-hq/crew --json jobs \
  --jq '.jobs[] | select(.name|startswith("Desktop Smoke E2E")) | {name, conclusion, startedAt, completedAt}'
```

## Steps

1. Write `docs/crew/verification/0007-gate-e2e-shard-relationship.md` following
   the numbering in `docs/crew/verification/` (0006 is the current tail). It
   records: the question, the four workflow/script/test citations above, the
   upstream contrast, the per-shard run table, and a verdict.
2. Fix the gap in `docs/crew/CI.md`: add `Desktop Smoke E2E` to the job table
   (`:15-23`) with "Runs when: desktop paths change" and "Proves: **nothing that
   blocks merge** — advisory, `continue-on-error`, excluded from the gate by
   design (#36/#37)". Add one sentence under "Merge contract" stating plainly that
   a green `NuncioCrew Gate` is not evidence that E2E passed — mirroring the
   existing honest boundary at `CI.md:66-70` ("A green merge gate is not release
   proof").
3. Do **not** change any workflow, script, or test in this phase.

## PASS / FAIL / INCONCLUSIVE (this phase is the evidence gate for Phase 04)

- **PASS** — the record cites the exact lines proving the gate excludes smoke, the
  contrast with upstream `ci.yml`, and a per-shard table covering at least the last
  10 `main` runs. `CI.md` no longer implies the gate covers E2E.
- **INCONCLUSIVE** — run history is unavailable or the #114 triage input is
  missing; record what is known, name the missing input, and let Phase 04 wait.
- **FAIL** — evidence contradicts the citations above (e.g. the gate *does*
  aggregate smoke on the live workflow). Then the issue's premise changes and
  Phase 04 must be re-planned before anyone acts.

## Validation

- Every claim in the record resolves to a `path:line` or a `gh run` id — the
  issue's bar: "reproducible: cites the workflow file lines and the concrete #114
  check-run evidence".
- `just ci` on the docs branch.
- The anti-drift rule from Phase 01 does not trigger here (no shipped-state
  change) — but if Phase 04 later changes the gate, that PR does trigger it.

## Risk and rollback

- **Risk:** the record reads as an accusation that the gate is broken. It is not —
  the posture is deliberate and recorded. Write it as "what the gate proves", per
  `AGENT-WORKING-AGREEMENT.md:18-23` (plain, no CEO brief).
- **Risk:** scope creep into fixing #109/#110. Out of scope — those are their own
  issues.
- **Rollback:** docs-only; revert.
