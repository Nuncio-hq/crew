# Verification 0007 — Gate and Desktop Smoke E2E relationship

- **Date:** 2026-08-10
- **Question:** Does a green `NuncioCrew Gate` prove that Desktop Smoke E2E
  passed?

## Workflow evidence

The answer is no. The Crew workflow documents the smoke suite as advisory:

- `.github/workflows/nuncio-crew-ci.yml:248-251` says the suite remains
  advisory while its moving failures are attributed and quarantined, keeps the
  signal visible with `continue-on-error`, and explicitly says not to add the
  job to `gate.needs` or `JOB_RELEVANCE` yet.
- `.github/workflows/nuncio-crew-ci.yml:252-253` names the job
  `desktop-smoke-e2e`; `.github/workflows/nuncio-crew-ci.yml:257` sets a
  30-minute timeout; `.github/workflows/nuncio-crew-ci.yml:258` sets
  `continue-on-error: true`; and `.github/workflows/nuncio-crew-ci.yml:262`
  runs shards 1 through 4.
- `.github/workflows/nuncio-crew-ci.yml:317` defines `gate`, and
  `.github/workflows/nuncio-crew-ci.yml:320` lists
  `needs: [changes, desktop-fast, desktop-rust, macos-arm, project-relay,
  buzz-acp]`; `desktop-smoke-e2e` is absent.
- `desktop/scripts/check-nuncio-crew-ci-results.mjs:6-12` has no
  `desktop-smoke-e2e` entry in `JOB_RELEVANCE`.
- `desktop/src/testing/nuncio-crew-ci-contract.test.mjs:130-156` contract-tests
  this posture. In particular, `:150` asserts that the workflow does not
  consume `needs.desktop-smoke-e2e.result`, and `:155` asserts that the gate
  helper has no `desktop-smoke-e2e` reference.

This is deliberate exclusion, not a mis-reported result. The inherited
upstream workflow does the opposite: `.github/workflows/ci.yml:295` includes
`desktop-smoke-e2e` in the gate's `needs`, and `.github/workflows/ci.yml:306-307`
fails the gate when that result is not success.

## Concrete #114 run

Run `31362178966` was the `NuncioCrew CI` run for `35af74019` (`#114`) on
2026-08-10. `NuncioCrew Gate` succeeded at `06:38:19`; Desktop Smoke E2E shard
1 failed, shard 3 failed, shard 2 succeeded, and shard 4 was cancelled at
`07:00:04` after starting at `06:29:47` (`gh run 31362178966`).

The strongest observation is that the gate completed 22 minutes before shard 4
finished (`gh run 31362178966`). The gate does not wait on the smoke shards or
consume their conclusions.

## Last 10 `main` runs

The per-shard conclusions below are from the last 10 `NuncioCrew CI` runs on
`main`. Each row is identified by its GitHub Actions run ID.

| Run | Head | Shard 1 | Shard 2 | Shard 3 | Shard 4 | Gate |
| --- | --- | --- | --- | --- | --- | --- |
| [`31362178966`](https://github.com/Nuncio-hq/crew/actions/runs/31362178966) | `35af74019` (#114) | failure | success | failure | cancelled | success |
| [`31351537772`](https://github.com/Nuncio-hq/crew/actions/runs/31351537772) | `06107122b` (#115) | skipped (docs-only) | — | — | — | success |
| [`31325507788`](https://github.com/Nuncio-hq/crew/actions/runs/31325507788) | `304173e42` (#113) | failure | success | failure | cancelled | success |
| [`31317585196`](https://github.com/Nuncio-hq/crew/actions/runs/31317585196) | `6793c86da` (#108) | failure | success | success | cancelled | success |
| [`31263949909`](https://github.com/Nuncio-hq/crew/actions/runs/31263949909) | `e41a1a6a4` (#107) | failure | success | success | cancelled | success |
| [`31258634798`](https://github.com/Nuncio-hq/crew/actions/runs/31258634798) | `a74a18fc3` (#106) | failure | success | success | cancelled | success |
| [`31256878409`](https://github.com/Nuncio-hq/crew/actions/runs/31256878409) | `820146681` (#103) | failure | success | success | cancelled | success |
| [`31253637576`](https://github.com/Nuncio-hq/crew/actions/runs/31253637576) | `bf9260544` (#101) | failure | success | success | cancelled | success |
| [`31251678176`](https://github.com/Nuncio-hq/crew/actions/runs/31251678176) | `f1b1eb485` (#100) | skipped (non-desktop) | — | — | — | success |
| [`31188797720`](https://github.com/Nuncio-hq/crew/actions/runs/31188797720) | `c1bffec27` (#98) | failure | success | success | cancelled | success |

Across the eight desktop-touching runs, shard 1 failed `8/8`, shard 4
cancelled at the 30-minute timeout `8/8`, shard 3 failed `2/8`, shard 2 passed
`8/8`, and `NuncioCrew Gate` succeeded `10/10` (`gh run 31362178966`,
`31325507788`, `31317585196`, `31263949909`, `31258634798`, `31256878409`,
`31253637576`, `31251678176`, `31188797720`, and `31351537772`).

Issue [#109](https://github.com/Nuncio-hq/crew/issues/109) records shard 4's
timing-out specs as upstream-owned at `desktop-v0.5.7`; the Crew-only
`project-outcomes.spec.ts` is not among them. Issue
[#110](https://github.com/Nuncio-hq/crew/issues/110) tracks shard 1's hard
failure at `channels.spec.ts:500`, which has failed since `25263120e` (#96).

The run history establishes the timeout pattern, but this record does not
claim a count of tests executed before shard 4 was killed. That count is not
needed to establish the gate relationship and is not present in the supplied
run conclusions.

## Open input

PR #114 merged without a separate published flake-versus-real triage of its
shard 1 and shard 3 failures (`gh run 31362178966`). Shard 1 matches the
`channels.spec.ts:500` signature tracked by issue
[#110](https://github.com/Nuncio-hq/crew/issues/110), which predates #114.
Phase 04 should treat this as the one open triage input; the workflow-config
half of this audit is complete.

## Verdict

**PASS.** The Crew gate excludes Desktop Smoke E2E by design, the exclusion is
contract-tested, and the upstream contrast is explicit. The defect found here
was the documentation gap: `docs/crew/CI.md:15-23` listed the gate jobs without
mentioning Desktop Smoke E2E. This record and the CI table now state that a
green `NuncioCrew Gate` is not E2E evidence.
