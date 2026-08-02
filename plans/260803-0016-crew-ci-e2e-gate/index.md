# Make the Playwright smoke suite a real merge gate

**Status:** planned, not started
**Issue:** [#36](https://github.com/Nuncio-hq/crew/issues/36)
**Base:** `main` after PR #35 merges — #35 adds specs this job must run
**Single phase.** One coherent change across three files that must move together.

## Outcome

A desktop PR that breaks a smoke spec fails `NuncioCrew Gate`. Today it merges
green.

## Why the wiring is three files, not one

The merge gate is contract-tested, so adding a job to the workflow alone breaks
`CI Policy` (which runs `node --test desktop/src/testing/nuncio-crew-*.test.mjs`).
All three move in one commit:

| File | Change |
|---|---|
| `.github/workflows/nuncio-crew-ci.yml` | new `desktop-smoke-e2e` job; add to `gate.needs` (line 159) and to the gate JSON (line 168) |
| `desktop/scripts/check-nuncio-crew-ci-results.mjs` | add the job to `JOB_RELEVANCE` with relevance key `desktop` |
| `desktop/src/testing/nuncio-crew-ci-contract.test.mjs` | job list at line 27; the gate cases at lines 85-141 enumerate jobs explicitly |

## Steps

**1. Decide the shape first — it changes the YAML.**
Default to **4 shards on `pull_request`**, mirroring upstream `ci.yml:226-290`.
That is the shape the suite was written for and the one that catches breakage
before merge. If CI minutes are the binding constraint, the fallback is
`push`-to-`main` only: same wiring, `if:` differs, rot is caught within a day
instead of at PR time. Owner decides in #36; do not guess silently — state which
one you implemented in the PR body.

**2. Copy the working job, do not invent one.**
Upstream `ci.yml:226-290` already solves Playwright browser install + cache +
sharding + artifact upload. Port it, adapting:
- `if: needs.changes.outputs.desktop == 'true'` (this fork's filter, already
  covers `desktop/**` and `.github/workflows/nuncio-crew-ci.yml`)
- hermit activation via `cashapp/activate-hermit`, as every job here does
- `pnpm exec playwright test --project=smoke --shard=${{ matrix.shard }}/4`
- keep `pnpm build:e2e` — a plain `pnpm build` strips the mock bridge and every
  spec fails with `Cannot read properties of undefined (reading 'invoke')`
  (`CLAUDE.md` § Writing E2E Screenshot Specs)

**3. Wire the gate.** `JOB_RELEVANCE` maps job → relevance key; `desktop` is the
right key. The existing gate semantics already say "must be `success` when
relevant, `skipped` when not" — no new logic needed, only registration.

**4. Prove it fails.** The acceptance criterion that matters: break a spec
deliberately on a scratch branch, push, and confirm `NuncioCrew Gate` goes red.
A green pipeline proves nothing about a gate — a gate that cannot fail is
decoration. Revert the break before opening the PR.

**5. Triage flake first — this is a prerequisite, not cleanup.**

Measured 2026-08-03, two full runs on the same machine, ~22 min each:

```text
clean main  5c79c8cc2   5 failed / 822 passed / 1 skipped
PR #35 tip  31607c7a3   9 failed / 819 passed / 1 skipped
```

The failing sets only partly overlap, and `main` failed two specs
(`community-rail:798`, `onboarding-agent-defaults:675`) that the PR tip passed. A
targeted rerun of the six tip-only failures under a quiet machine passed five of
six. So the suite's steady state on green code is roughly **5 failures per run,
with the identity of the failures moving between runs**.

**Turning this job on as a blocking gate today would red-wall every PR.** The
order therefore matters:

1. Attribute each recurring failure. Known repeat offenders: `relay-reconnect:135`,
   `video-attachment:229`, and the `composer-selection-formatting:203` parametrized
   family (a *different* case fails each run, on `main` as well as on feature tips).
2. Fix, or quarantine out of the `smoke` project with a comment naming a tracking
   issue — quarantine is honest, a blanket `retries: N` is not. Retries convert a
   real failure into a slow one and are how the suite lost its meaning.
3. Only then flip the job to blocking, and prove it fails (step 4).

If quarantining the flaky set is more work than the gate is worth tonight, land
the job **non-blocking** (`continue-on-error: true`, not in the gate JSON) and say
so plainly in the PR — a visible-but-advisory signal beats both a fake gate and
nothing. Do not register a job in `JOB_RELEVANCE` that cannot be trusted to fail
for real reasons.

## Non-goals

- Re-enabling upstream `ci.yml`. It carries relay/Rust/mobile jobs this fork does
  not want. Port the one job.
- Rewriting or restructuring specs. If a spec is broken, that is its own issue.
- Adding e2e to the release workflow.

## Acceptance

- [ ] Smoke runs in `NuncioCrew CI` on desktop-touching PRs (or on `main` push,
      if that is the owner's call in #36 — say which)
- [ ] Deliberately broken spec turns `NuncioCrew Gate` red, verified on a real run
- [ ] `node --test desktop/src/testing/nuncio-crew-*.test.mjs` green
- [ ] `relay-reconnect` and `video-attachment` each attributed, with a linked
      issue for anything deferred
- [ ] No blanket retry count added
