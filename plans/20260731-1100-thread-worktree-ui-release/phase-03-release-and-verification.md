# Phase 03 — Release and Verification

## Overview

Priority: high. Status: in progress.

Make Crew release identity independent of inherited Buzz tags, then publish and
verify the stable updater path.

## Release Contract

- Workflow input remains Crew app version syntax (`v0.0.5`).
- Helper derives immutable GitHub tag `crew-v0.0.5`.
- Artifact version and `latest.json.version` remain `0.0.5`.
- Archive URLs point to the Crew-prefixed immutable release.
- Rolling tags remain `nuncio-crew-stable-latest` and
  `nuncio-crew-dev-latest`.

## Related Files

- `.github/workflows/nuncio-crew-release.yml`
- `desktop/scripts/nuncio-crew-release-channel.mjs`
- `desktop/scripts/generate-nuncio-crew-latest-json.mjs`
- `desktop/src/testing/nuncio-crew-release-contract.test.mjs`
- `docs/crew/RELEASING.md`
- `docs/crew/STATE.md`
- `docs/crew/CHANGELOG.md`

## Verification

1. Run focused Rust, desktop unit, E2E integration, lint, format, and build.
2. Run visual comparison and record `final result: passed` in `design-qa.md`.
3. Obtain independent tester and code-reviewer reports.
4. Commit with DCO signoff, push, open ready PR, fix CI, and merge.
5. Dispatch stable `v0.0.5` release from the exact merged `main` SHA.
6. Verify release artifacts, signature, notarization, rolling manifest, and
   actual `0.0.4 → 0.0.5` updater relaunch without replacing application data.

## Success Criteria

- An inherited `v0.0.5` tag cannot block Crew `0.0.5`.
- Equal/lower manifest guard remains enforced.
- Stable never receives dev-only updates.
- Installed identity and data directory stay unchanged across the update.

## Unresolved Questions

None.
