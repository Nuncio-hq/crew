# Buzz Desktop 0.5.22 integration

Status (2026-09-05): all 191 source dispositions accounted for; local `just ci`,
dependency policy and required integration lanes passed. Final clean-bundle browser
acceptance passed; remote PR/manual gates remain pending. Working-branch metadata names
the target below.
This is not a merge, release, deployment, or hosted-provider acceptance record.

## Release boundary

Target: `desktop-v0.5.22`, commit
`9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`.
The [coverage ledger](../../../plans/260905-buzz-0-5-22-upgrade/coverage-ledger.csv)
accounts for all 191 commits after the previous recorded pin,
`39f8b46935736334cdd7045a4e4b5d7eb1a33888`.
The actual merge ancestor was
`4749bc7be3cdb78c2db4ce4864775ba7ab60b4cc`; the prior version label alone was not
proof that every earlier feature existed. Native, renderer, and supporting-source
inventories therefore checked final released bodies and missing dependencies.
No post-0.5.22 behavior is claimed.

## Integration and Crew boundaries

- Relay, auth, DB, search, CLI, web, mobile, native desktop, and renderer absorb
  the released source. Admin UI, deployment charts, benchmark corpus, protocol
  docs, and the countdown example are included in the supporting inventory.
- Crew remains channel-first (D-066): no Projects or Workbench rail. Released
  repository/project components use the existing capability-gated channel and
  outcome surfaces. Exact linked-folder authority and read-only local-source
  boundaries remain.
- One agent can serve many independent threads. Crew retains conversation UUID,
  resume-first session/ledger, checkout leases, and raw-root identity. Released
  aggregate per-routing-channel queue bounds and cancellation correlation are
  adapted onto those existing models; no channel-default identity migration.
- Hermes remains first-class with exact profile ownership and write-through.
  Explicit profile links stay on the requested key; persona navigation chooses
  a representative separately. Effort and access policy persist through Save;
  selection and Cancel do not write them.
- Presence describes availability, not process ownership. Start/Restart retain
  Crew lifecycle authorization and are not disabled merely by Online/Away.
- Keyboard zoom remains text-only. The 13/14/15 conversation font contract,
  fixed layout coordinates, Crew theme/chrome/syntax treatment, and runtime
  avatar assets remain. File-size limits remain 1000 with explicit existing
  baselines; no upstream surface-limit increase was adopted.
- All existing Crew migrations through 0033 retain their applied bytes.
  Released SQL appends as Crew 0034–0045, including final NIP-FI ledger removal.
  Desired-state schema reconciliation and isolated PostgreSQL test discovery
  are wired into Crew CI.
- Crew keeps NuncioCrew Gate and its manual release workflow. Upstream-only
  deployment, staged-image, and automated security-review workflows are not
  enabled. Private vulnerability reporting is unavailable pending maintainer
  setup; see [SECURITY.md](../../../SECURITY.md). No external setting changed.

## Final local gates

`just ci` completed with exit 0 in `/tmp/crew-upgrade-just-ci-final-2.log`:
formatting, checks, builds and tests passed. Its desktop selection passed 6991 tests
with one skip; Tauri passed 3431 with 19 ignored, plus the remaining workspace
targets; mobile passed 2086. The final desktop rerun includes the wheel-retry regressions: **6996 passed, one
skipped, zero failures** out of 6997 tests in
`/tmp/crew-upgrade-desktop-test-final.log`.

`cargo-deny` passed advisories, bans, licenses and sources in
`/tmp/crew-upgrade-cargo-deny.log`. Existing non-fatal warnings remain visible in
the log; success does not mean warning-free output.

The [integration gate report](../../../plans/reports/tester-260905-full-integration-gate.md)
records 126 DB tests, 379 isolated PostgreSQL tests and 314 workspace integration
tests passed. A final modified huddle query and three strict metrics source guards
also passed. Counts are per lane and can overlap. Scratch database isolation and
cleanup are documented there; no shared database or deployment was changed.

## Integration regressions resolved

- PR comment refresh could loop indefinitely because an equal-valued PR reference
  changed object identity each render. The store now stabilizes owner/repository/
  number; invalidation and explicit refresh share the existing in-flight request.
  Nine focused tests passed. The same
  [independent report](../../../plans/reports/debugger-260905-pr-hub-comment-refresh-loop.md)
  verifies visible management provenance on member hover, a filling tool pane at
  narrow widths, and released Bestie shadow values through Crew color tokens.
- Automatic model inheritance, thread heading structure, macOS-only glass and PR
  tool-tab selection were independently reviewed with 79 focused tests passed.
  [Final integration review](../../../plans/reports/reviewer-260905-final-integration-fixes.md).
- A clean 10,000-row browser replay exposed pagination stranded at the hard top
  after a temporarily blocked boundary callback. Negative wheel input now retries
  the existing guarded callback even when no scroll event occurs. The settle gate,
  search/fetch checks and momentum suppression remain. Five mounted regressions
  passed; final clean-bundle deep replay passed all 10,000 ids.
  [Wheel retry review](../../../plans/reports/reviewer-260905-timeline-wheel-retry.md).
- Strict integration checks exposed missing operation attribution in workflow
  event reads and Crew's batched huddle-link authorization query. Existing query
  bodies and tenant boundaries remain; final compile, metrics guards and database
  regressions passed in the integration report above.

## Additional subsystem evidence

| Surface | Recorded evidence | Source |
|---|---|---|
| ACP/workflow | 1175 library, 14 integration and 6 PostgreSQL regressions passed | [ACP report](../../../plans/reports/implementer-260905-acp-priority-ports.md) |
| Projects | 69 focused protocol/contributor/folder-safety tests passed | [Projects report](../../../plans/reports/260905-projects-release-integration.md) |
| Profile/agents | 1170 scoped tests passed | `/tmp/crew-profile-agent-final-tests.log` |
| Relay client integration | 59 publish/replay/transport tests passed | [Independent review](../../../plans/reports/reviewer-260905-renderer-integration.md) |
| Supporting sources | Admin 28 browser; benchmarks 91; testbed 39 + 1 live skip; Helm 47 passed | [Supporting report](../../../plans/reports/implementer-260905-supporting-source-ports.md) |

Supporting source review compared 198 target files: 195 exact, three justified
test/docs adaptations, zero missing. Python tests used publicly resolved declared
development dependencies because the upstream lock registry was unavailable.
No frozen-lock or deployment claim follows.

The [source audit](../../../plans/reports/reviewer-260905-coverage-audit.md) records
25 ported, 154 adapted and 12 retained Crew dispositions with zero pending rows.
Its refreshed filesystem snapshot has 1428 target-exact, 350 adapted and 42
intentional Crew/mapped paths across 1820 unique paths.

The [manual sync workflow](../../../plans/reports/reviewer-260905-manual-upstream-gate-readiness.md)
now includes the existing Crew Linux native setup, CMake compatibility and pinned
nextest. Independent review and 14 workflow contract tests passed; no remote run
has occurred yet.

## Remaining acceptance

Final clean-bundle release smoke passed **3/3**: all **10,000 ids** reachable with
an exact expected hash, **199 continuations** across 200 pages, maximum **164**
mounted rows, and zero duplicates, ordering errors or render-pending timeouts.
The final cursor is exhausted. No diagnostic probe or recovery nudge was present.
Four distinct visual evidence states and focused browser results are recorded in
the [browser report](../../../plans/reports/tester-260905-desktop-release-browser-evidence.md).

The PR has not been created; remote NuncioCrew Gate and manual upstream-sync
execution remain pending. Root coordinator owns commit, push, PR and merge. No merge, release,
deployment or hosted-provider acceptance is claimed. Hosted Hermes and receipt
compatibility limits for issues #337/#338 remain recorded separately in
[STATE.md](../STATE.md).
