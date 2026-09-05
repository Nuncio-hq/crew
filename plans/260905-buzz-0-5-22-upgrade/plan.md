# Buzz Desktop 0.5.22 integration plan

Status (2026-09-05): source integration and local gates complete;
Local candidate evidence is recorded below; full NuncioCrew CI and manual upstream compatibility must pass at merge. No merge, release or hosted acceptance claimed.

Target: `desktop-v0.5.22` / `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`.
Prior recorded pin: `39f8b46935736334cdd7045a4e4b5d7eb1a33888`.
Actual shared ancestor: `4749bc7be3cdb78c2db4ce4864775ba7ab60b4cc`.

## Scope and seams

Extend existing Buzz relay events, stores, client types and native commands.
Preserve Crew channel-first routing, UUID/resume-first parallel thread sessions,
Hermes profile ownership, text-only zoom, theme identity and the 1000-line guard.
Keep applied Crew migrations through 0033 unchanged; released SQL appends 0034–0045.

## Progress

- [x] Inventory all 191 upstream commits and final released source, including
  missing dependencies older than the recorded pin.
- [x] Integrate backend/auth/DB/search/CLI, ACP/workflow, desktop native/renderer,
  mobile/web, Admin UI, deployment charts, benchmark corpus and protocol docs.
- [x] Classify every commit: 24 ported, 155 adapted, 12 retained Crew divergences;
  zero pending. [Coverage ledger](coverage-ledger.csv).
- [x] Independently review Crew-specific boundaries and final integration fixes:
  PR refresh loop, member provenance, narrow tool pane, automatic models, platform
  glass, thread headers, query metrics and hard-top wheel retry.
- [x] Run local `just ci`: checks/builds passed; desktop 6991 passed + one skip,
  Tauri 3431 passed + 19 ignored, mobile 2086 passed. Final desktop rerun includes
  browser reconciliation: 7031 passed + one existing skip, zero failures (7032 total, 98.23s).
- [x] Run dependency policy: advisories, bans, licenses and sources OK.
- [x] Run required integration selections in isolated infrastructure: 126 DB,
  379 PostgreSQL and 314 workspace integration tests passed, plus final metrics
  guards and modified huddle query. Counts are per lane and overlap.
- [x] Prepare manual sync workflow with existing Crew Linux dependencies,
  CMake compatibility and pinned nextest; review and 14 contract tests passed.
- [x] Final clean-bundle release smoke: 3/3 passed, exact 10,000-id hash,
  199 continuations, maximum 164 mounted rows, no duplicate/order/render-pending errors.
- [x] Open PR #342; Gate job and manual sync passed on `0ef5491f`. The encompassing
  CI workflow was cancelled after smoke shards 3/4 timed out with failures.
- [x] Verify browser repairs: full Projects 8/8, huddle 24/24, real relay
  17/17; channel selection 13 and mounted guard four passed.
- [x] Reconcile late browser findings: immutable aging snapshots, repository selector,
  contextual thread reading/home navigation, sidebar visibility and search height;
  faithful native publication fixture and independently gated concurrent installs.
- [x] Independently run final desktop unit suite and review late source changes;
  browser evidence is linked per selection rather than summed across overlapping suites.
- [x] Final source-freeze checks (all guards), TypeScript and diff checks passed.
- [x] Fix unacknowledged prepend compensation; installed ESM/CJS 19/19 and full
  macOS/Linux virtualization suites 11/11 each passed with zero retries.
- [ ] Shipping gate: full NuncioCrew CI (every desktop smoke shard) and manual
  upstream compatibility must pass on the exact PR head at merge.
- [ ] Coordinator completes merge/release decisions only after required gates.

## Evidence and remaining boundaries

[Integration record](../../docs/crew/verification/upstream-0-5-22-integration.md),
[source audit](../reports/reviewer-260905-coverage-audit.md),
[full integration](../reports/tester-260905-full-integration-gate.md),
[PR refresh review](../reports/debugger-260905-pr-hub-comment-refresh-loop.md),
[wheel retry review](../reports/reviewer-260905-timeline-wheel-retry.md), and
[manual gate readiness](../reports/reviewer-260905-manual-upstream-gate-readiness.md).

Hosted Hermes and receipt acceptance for #337/#338 remains a separate incomplete
record in [Crew State](../../docs/crew/STATE.md). No external settings, deployment,
private-reporting enablement or hosted-provider success follows from local tests.

Filesystem census refreshed from actual bytes: 1410 target-exact, 368 adapted,
42 intentional Crew/mapped paths; all 191 commit dispositions remain 24/155/12.

Shipping evidence: [PR #342 checks](https://github.com/Nuncio-hq/crew/pull/342/checks).
