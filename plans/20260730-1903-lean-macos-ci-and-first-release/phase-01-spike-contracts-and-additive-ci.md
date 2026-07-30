# Phase 01 — Spike, contracts, and additive CI

## Context links

- [Plan](plan.md)
- [`docs/crew/spikes/0008-lean-macos-arm-ci.md`](../../docs/crew/spikes/0008-lean-macos-arm-ci.md)
- [`desktop/src/testing/nuncio-crew-release-contract.test.mjs`](../../desktop/src/testing/nuncio-crew-release-contract.test.mjs)

## Overview

Priority: blocking. Status: complete; RED contracts captured and additive
workflows implemented locally.

Prove a minimal unsigned macOS ARM package, lock the intended GitHub Actions
surface in RED tests, then add Crew-owned workflows without editing upstream.

## Spike and baseline

1. Record PR #1 inherited failures with run/job links and exact errors:
   - `Desktop Build (macOS)`: missing `mesh-llm` checkout after `cargo fetch`.
   - four Docker build checks: denied writing Block-owned cache/registry data.
2. Confirm the Tauri default build excludes optional `mesh-llm`.
3. Build an unsigned Apple Silicon package without Apple or updater secrets.
4. Verify the app architecture, Nuncio product metadata, real or deliberately
   identified placeholder sidecars, and updater-disabled local configuration.
5. Record the spike result in `docs/crew/spikes/0008-lean-macos-arm-ci.md`.

## TDD contract

Add `desktop/src/testing/nuncio-crew-ci-contract.test.mjs` first and capture a
RED run caused only by the absent Crew workflows. The tests must assert:

- `NuncioCrew CI` runs on `pull_request` and pushes to `main`, never
  `pull_request_target` or a schedule.
- top-level permissions are read-only and automatic jobs reference no signing,
  Apple API, publication, Docker, mobile, Windows, Linux, Helm, or mesh inputs.
- job/check names are exactly `Desktop Fast`, `macOS ARM Package`,
  `Project Relay`, and the always-present `NuncioCrew Gate`.
- the final gate uses `always()` and accepts a deliberately skipped Project job
  but rejects failed or cancelled dependencies.
- Project relay coverage is path-filtered and runs the existing live-relay test.
- `NuncioCrew Upstream Sync` is `workflow_dispatch` only and retains the
  heavyweight format, clippy, unit, and dependency-policy checks.
- inherited workflow files are not referenced as edit targets.

Recorded RED evidence: all four contract cases fail with `ENOENT` because
`nuncio-crew-ci.yml` and `nuncio-crew-upstream-sync.yml` do not exist. This is
the intended pre-implementation failure boundary; no product assertion failed.

## Implementation

1. Add `.github/workflows/nuncio-crew-ci.yml`.
2. Add `.github/workflows/nuncio-crew-upstream-sync.yml`.
3. Keep automatic workflow permissions read-only: `contents: read` and
   `pull-requests: read`.
4. Use frozen dependency installs and the repository Hermit toolchain.
5. Build only `aarch64-apple-darwin`, unsigned, using the Nuncio release config
   stack without reading protected release secrets.
6. Make `NuncioCrew Gate` depend on `CI Policy` and all three supporting jobs.
7. Re-run the contract test GREEN.

## Local verification

```text
cd desktop
node --import ./test-loader.mjs --experimental-strip-types --test \
  src/testing/nuncio-crew-ci-contract.test.mjs \
  src/testing/nuncio-crew-local-build-contract.test.mjs \
  src/testing/nuncio-crew-release-contract.test.mjs
pnpm typecheck
pnpm check
pnpm build
```

Also run YAML parsing, `bash -n` for invoked shell helpers, the unsigned package
command used by CI, package metadata/architecture assertions, and
`git diff --check`.

## Success criteria

- RED and GREEN outputs are captured.
- No existing workflow changes.
- No secret name is resolved to a value.
- Local unsigned package verification passes on Apple Silicon.
- Exact check names are stable enough for branch protection.

## Risks and rollback

If package time or reliability is unacceptable, retain the RED contract and
reduce only redundant build work; do not remove the package proof. Roll back by
deleting the two additive workflows and their contract test.

## Unresolved questions

None.
