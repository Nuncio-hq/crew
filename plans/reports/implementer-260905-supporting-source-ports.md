# Supporting upstream source ports — 2026-09-05

Status: DONE. Docs impact: minor (subtree READMEs and compose configuration reference).

## Source coverage

Imported 124 paths from immutable `desktop-v0.5.22` (`9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`) within `admin-web/`, `benchmarks/`, `deploy/charts/`, `deploy/compose/.env.example`, and `examples/countdown-bot/src/main.rs`. Initial coverage report listed 56 gaps; direct source comparison found 68 additional missing benchmark dependencies from the earlier baseline (fixture bodies, verifiers, manifests, runtime and dependency declarations). Importing only the changed task definitions would have left an incomplete test corpus. No Crew commit or deletion history justified leaving those source dependencies absent.

Final byte comparison covers all 198 target files in the assigned directories: 195 target-exact, three deliberate differences listed below, zero missing target files. Inventory: `/tmp/crew-support-final-source-audit.json`; imported path list: `/tmp/crew-support-all-paths.json`.

- Admin: NIP-98 credential flow, role-dependent feedback writes, MIME-constrained attachments, auth retry/CSP tests, responsive feedback filters.
- Benchmarks: wait for scripted delivery, evaluation layers, cold-memory fixture, complete Buzz-native dataset and verifier/runtime dependencies.
- Relay chart: versioned S3 deletion configuration, immutable digest selection, KLIPY secret injection, updated readiness/database documentation and tests.
- Push chart: gateway deployment/monitoring contract, Datadog fixtures, schema validation, release contract and rendering tests.
- Countdown bot: seventh message-builder argument for emoji tags, matching current SDK.

## Preserved adaptation and test corrections

1. `deploy/charts/buzz/README.md`: preserved Crew roster-fence migration **0033**, where upstream documents 0032. Verified existing Crew HEAD delta before import.
2. `admin-web/tests/auth.spec.ts`: two attachment fixtures now return the served `image/png` or `text/plain` MIME matching their expected rendering. Upstream fixtures returned `application/octet-stream` while expecting inline images, contradicting the released MIME-safety gate. Production remains target-exact; hostile octet-stream download-only coverage remains passing.
3. `admin-web/tests/routes.spec.ts`: same served-MIME fixture correction; replaced stale local-storage/checkbox status assertion with relay-status filtering and reload coverage. Released status writes are server-backed; new signed PATCH success/failure and disabled-mode read-only tests remain intact and pass.

## Validation

- Admin `pnpm build`: TypeScript + Vite passed (`/tmp/crew-support-admin-build.log`).
- Admin Biome check: 16 files passed (`/tmp/crew-support-admin-lint.log`).
- Admin Playwright: **28 passed** (`/tmp/crew-support-admin-e2e.log`).
- Admin Vitest command succeeds with no source unit tests; browser suite above provides behavior coverage.
- Orchestra pytest: **91 passed** (`/tmp/crew-support-benchmark-tests.log`).
- Testbed pytest: **39 passed, 1 skipped** (`/tmp/crew-support-testbed-tests.log`); skipped case requires a running live testbed stack.
- Ruff: passed; Python compileall: passed (`/tmp/crew-support-benchmark-lint.log`).
- Helm 3.16.4 / helm-unittest 0.8.2 (same pinned versions as repo CI): **47 tests, 9 suites passed** (`/tmp/crew-support-helm-tests.log`).
- Relay chart dependencies built; lint including subcharts passed; every CI/test values fixture rendered successfully.
- Push gateway chart lint/render guard passed (`/tmp/crew-support-push-render.log`); release-contract script passed (`/tmp/crew-support-push-release-contract.log`). Neither command publishes or deploys.
- Countdown bot `cargo check --manifest-path examples/countdown-bot/Cargo.toml`: passed (`/tmp/crew-support-countdown-check.log`).
- Scoped `git diff --check`: passed.

Python validation environment: checked-in upstream lockfiles resolve through Block's private artifact registry, unavailable here. Preserved lockfiles exactly; installed declared development dependencies into isolated `/tmp/crew-support-python` via public PyPI for the actual tests. This validates source behavior but is not a frozen-lock reproducibility claim.

No index, commit, push, release workflow, deployment, root pin, or STATE changes by this worker. Root owns integration and broad final gate. Independent planner review passed: all three target deviations match current production contracts and Crew migration mapping; no actionable findings.

Unresolved questions: none.
