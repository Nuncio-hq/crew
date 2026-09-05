# Manual upstream gate prerequisites — 2026-09-05

Status: DONE. Independent review found no issues. Docs impact: this readiness report only.

## Findings and narrow fix

`.github/workflows/nuncio-crew-upstream-sync.yml` runs workspace Rust checks and native Tauri clippy/tests on Ubuntu, but previously installed no native development libraries. The existing Crew `desktop-rust` job already supplies the Linux prerequisites required by those same Tauri recipes.

Copied its Linux dependency step exactly, including noninteractive apt, retry/timeouts, lock wait, and packages: build-essential, curl, file, ALSA, Ayatana appindicator, GTK3, librsvg, OpenSSL, WebKit2GTK 4.1, xdo, patchelf and wget. This runs before workspace compilation because the workspace also contains native audio dependencies.

Set compatibility-job `CMAKE_POLICY_VERSION_MINIMUM=3.5`, matching the native Crew job. Hermit pins CMake 4.3.1; this carries Crew's existing compatibility setting into both workspace and Tauri native dependencies.

Installed the same pinned cargo-nextest 0.9.136 through the same pinned installer action already used by Crew PostgreSQL CI. `Justfile:test-unit` selects nextest when available. Its full branch includes buzz-auth compile-fail doctests, full buzz-agent integration targets and scoped relay admin regressions; the fallback runner does not enumerate all of those. Supplying nextest preserves the intended gate coverage instead of silently selecting the smaller fallback on a fresh runner.

All existing format, clippy, unit, Tauri and dependency-policy commands remain unchanged. Workflow remains manual-only, read-only permissions, no new Block lane or automated security review. Timeout and concurrency policy remain unchanged.

## Other prerequisites checked

- Hermit provides pinned Rust, Node, pnpm, CMake, just and cargo-deny. No separate language-version setup needed.
- This workflow runs Rust-only recipes; it does not run desktop JS tests, TypeScript or frontend build. `scripts/test-ensure-local-relay-key.sh` calls Node's built-in crypto module only. No guaranteed missing npm dependency was found, so no unrelated pnpm installation was added.
- Both Tauri compilation recipes already depend on `_ensure-sidecar-stubs`; no extra placeholder step is needed. These are existing compile-time placeholders, not packaging or execution evidence.
- The unit gate is infrastructure-free; PostgreSQL/Redis-backed tests have their separate Crew lane. No fake service or skipped native test was introduced.

## Validation

- Parsed workflow YAML with installed YAML parser.
- Structurally compared the copied Linux install step against Crew `desktop-rust`: exact equality.
- Asserted CMake compatibility value and sole workflow_dispatch trigger.
- Scoped `git diff --check`: passed.
- Independent reviewer verified the final diff preserves all gates and adds only prerequisites; no findings. Existing `nuncio-crew-ci-contract.test.mjs`: 14/14 passed.

No Ubuntu runner or workflow dispatch executed locally. This report removes source-proven missing setup; actual Linux execution remains the manual CI gate.

Unresolved: remote Ubuntu execution remains the manual CI gate.
