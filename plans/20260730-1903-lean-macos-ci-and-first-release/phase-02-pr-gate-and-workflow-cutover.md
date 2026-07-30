# Phase 02 — PR gate and workflow cutover

## Context links

- [Plan](plan.md)
- [Phase 01](phase-01-spike-contracts-and-additive-ci.md)
- [PR #1](https://github.com/Nuncio-hq/crew/pull/1)

## Overview

Priority: blocking. Status: pending.

Prove the additive gate on PR #1, merge using the Crew-owned signal, then switch
off inherited automation without modifying its source files.

## PR green gate

1. Push the additive workflows and contract test to PR #1.
2. Wait for these expected checks:
   - `Desktop Fast` — success.
   - `macOS ARM Package` — success.
   - `Project Relay` — success when relevant paths changed, otherwise skipped.
   - `NuncioCrew Gate` — success in both Project outcomes.
3. Confirm the gate helper's unit contract rejects failed, cancelled, and
   missing dependencies.
4. Treat inherited failures as documented, non-required upstream signals; do
   not suppress them by editing their YAML.
5. Configure the repository ruleset to require exactly `NuncioCrew Gate`.
6. Merge only after focused local verification and the required check are green.

## Repository workflow cutover

1. Verify a post-merge `NuncioCrew CI` run on the exact `main` SHA.
2. In GitHub Actions settings, disable inherited automatic workflows, including
   current `CI` and `Docker image`, while leaving their files intact.
3. Keep enabled:
   - `NuncioCrew CI`
   - `NuncioCrew Release`
   - `NuncioCrew Upstream Sync`
4. Inspect the ruleset again and confirm no disabled inherited job is required.
5. Dispatch `NuncioCrew Upstream Sync` once; record results as compatibility
   evidence, not as a release or merge blocker.

## Success criteria

- Required check context is exactly `NuncioCrew Gate`.
- `main` has a successful `NuncioCrew CI` run.
- Inherited workflow files have no diff.
- GitHub shows inherited workflows disabled at repository level.
- Re-enabling the inherited workflows remains a one-click/API rollback.

## Risks

- GitHub may expose the required context with a workflow prefix. Capture the
  exact context from the first real run before saving the ruleset.
- Disabling a workflow does not remove its old failed runs from PR history.
  “Green gate” means every required Crew check passes, not that historical
  inherited rows disappear.

## Rollback

Re-enable inherited workflows first, remove the `NuncioCrew Gate` requirement
if it is unavailable, and disable `NuncioCrew CI`. Revert only the additive CI
commit after legacy checks are running again.

## Unresolved questions

None after the first live run reveals GitHub's exact required-check context.
