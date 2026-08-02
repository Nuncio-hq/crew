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

**5. Triage flake, do not paper over it.** A local run at `31607c7a3` had
failures in `relay-reconnect.spec.ts` and `video-attachment.spec.ts`. Before this
job becomes blocking, each needs attribution: real bug, environment-dependent, or
genuinely flaky. Options in order of preference — fix it, mark it
`test.fixme()` with a linked issue, or exclude it from the `smoke` project with a
comment naming the issue. **Do not** add a blanket `retries: N` to make the suite
quiet; that converts a real failure into a slow one and is how the suite became
untrustworthy in the first place.

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
