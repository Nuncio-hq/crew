# Buzz Desktop 0.5.22 integration plan

Status (2026-09-05): source integration, local gates and final browser complete;
remote PR/manual gates pending. No merge, release or hosted acceptance claimed.

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
- [x] Classify every commit: 25 ported, 154 adapted, 12 retained Crew divergences;
  zero pending. [Coverage ledger](coverage-ledger.csv).
- [x] Independently review Crew-specific boundaries and final integration fixes:
  PR refresh loop, member provenance, narrow tool pane, automatic models, platform
  glass, thread headers, query metrics and hard-top wheel retry.
- [x] Run local `just ci`: checks/builds passed; desktop 6991 passed + one skip,
  Tauri 3431 passed + 19 ignored, mobile 2086 passed. Final desktop rerun includes
  the wheel regressions: 6996 passed + one skip, zero failures.
- [x] Run dependency policy: advisories, bans, licenses and sources OK.
- [x] Run required integration selections in isolated infrastructure: 126 DB,
  379 PostgreSQL and 314 workspace integration tests passed, plus final metrics
  guards and modified huddle query. Counts are per lane and overlap.
- [x] Prepare manual sync workflow with existing Crew Linux dependencies,
  CMake compatibility and pinned nextest; review and 14 contract tests passed.
- [x] Final clean-bundle release smoke: 3/3 passed, exact 10,000-id hash,
  199 continuations, maximum 164 mounted rows, no duplicate/order/render-pending errors.
- [ ] Coordinator commits/pushes and creates the Crew PR; run remote NuncioCrew
  Gate and manual upstream-sync workflow against the actual branch revision.
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

Unresolved: remote gate outcomes.
