# Release coverage audit — 2026-09-05

**Status: DONE.** All 191 commits in the recorded upstream interval have a final source disposition. This is source coverage, not repository-wide acceptance, merge, release, deployment, or hosted-provider validation.

## Boundary and method

- Target `desktop-v0.5.22`: `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`.
- Previous recorded pin: `39f8b46935736334cdd7045a4e4b5d7eb1a33888`.
- Actual shared ancestor: `4749bc7be3cdb78c2db4ce4864775ba7ab60b4cc`.
- Pre-integration Crew HEAD: `871eecb18d7a243d87ec56a2eb154fbf2099d7ce`.
- Read each commit's touched-path inventory, group by final released source, and reconcile against subsystem implementation/review reports. Compare current filesystem bytes, including untracked additions, against target and Crew HEAD with `git cat-file --batch`. Comparing only `git diff` would incorrectly mark untracked imports as missing.
- The [ledger](../260905-buzz-0-5-22-upgrade/coverage-ledger.csv) contains 191 unique SHAs: **24 ported, 155 adapted, 12 retained-Crew-divergence, zero pending**. No row was labeled already-equivalent without whole-row proof.
- The [machine-readable source snapshot](260905-final-source-coverage.json) accounts for 1820 unique ledger paths: **1420 target-exact, 358 adapted, 42 unchanged Crew**. Target-exact includes paths deleted in both final target and Crew. Adapted is a byte comparison, not an independent correctness verdict; the ledger attaches subsystem evidence and concrete current paths.
- Final source takes precedence over intermediate commits that are superseded within the interval, including intermediate release versions and the NIP-FI authority ledger subsequently removed upstream.
- Source inventory was extended beyond this interval where missing pre-pin dependencies were found. Supporting-source inventory covered 198 released files and imported 68 pre-pin benchmark dependencies; native and renderer inventories likewise checked final target bodies.

## Every unchanged Crew path is accounted for

| Paths | Disposition and proof |
|---|---|
| 13 `.github/workflows/*` and two Codex security-review scripts | Preserve Crew CI/manual release ownership; no Block staging, deployment or automated reviewer activation. Crew gate remains active, stock release has a Block repository gate, and live CI/docker lanes are disabled. Shared production/test tooling is integrated separately. |
| `bin/actionlint`, `.actionlint-1.7.12.pkg` | Sole target usage is the intentionally omitted Codex security-review workflow lane. |
| `.release/desktop-candidate.json` | Preserve Crew release attribution; final working-branch package/pin metadata targets 0.5.22 without claiming release acceptance. |
| ACP `scope.rs`, `session_model_channel.md`, `session_model_thread.md` | Crew already owns UUID/resume-first conversation identity in `crates/buzz-acp/src/conversation.rs`. Released queue bounding, cancellation and prompt changes integrate into that seam. |
| `desktop/src-tauri/src/app_state_accessors.rs` | Existing Crew `app-state-accessors.rs` carries released accessors and signed replay-floor behavior. |
| Four zoom/font files | Byte-identical to Crew HEAD; explicit text-only zoom and fixed layout geometry retained. Independently reviewed in [final integration review](reviewer-260905-final-integration-fixes.md). |
| `useMembersSidebarActions.ts`, `useAgentLifecycleActions.ts` | Generic presence-based Start restrictions are intentionally absent: Crew supports one agent across concurrent independent threads. Availability display is ported, lifecycle authorization preserved. |
| `ProjectsCreateMenu.tsx` | Existing Crew repository menu remains behind channel-first capability/outcome routing. No Projects/Workbench rail. |
| `buzz-theme-screenshots.spec.ts` | Buzz-only theme screenshot suite remains removed; Crew theme and appearance coverage are retained. |
| 12 target migrations 0033–0044 | All twelve mapped SQL files in Crew 0034–0045 were independently byte-compared to target. Existing Crew migration history through 0033 remains preserved. |

Other deliberate adaptations: 1000-line guard and exact existing baselines; Crew theme, settings dropdowns, syntax treatment, runtime avatars, and 13/14/15 conversation font contract; Hermes ownership and Save/Cancel semantics; native macOS-only glass behavior; repository/outcome features on Crew's channel-first surface. Rich/compact preview controls now include the released live sample using Crew presentation components. Contrast and plain-text sample behavior are covered by browser checks.

Private vulnerability reporting was verified disabled for `Nuncio-hq/crew` through GitHub's read-only repository API. `SECURITY.md`, issue templates and contributor links now state the actual availability instead of sending Crew vulnerabilities to Block or inventing a private contact/SLA. Enabling private reporting or naming a private recipient remains maintainer setup, not an unresolved source port.

## Gaps found and resolved

The [resolved gap report](reviewer-260905-coverage-gaps.md) records missing Admin UI, chart, benchmark, documentation and example dependencies, subsequently imported and reviewed. Narrow public APNS fixture allowlisting was imported from target. Protected E2E restores the released `VITE_BUZZ_BESTIE=1` build flag; production default is unchanged.

Subsystem evidence is linked per ledger row. Key reports: [substrate](260905-substrate-integration.md), [native](implementer-260905-native-release-integration.md), [ACP](implementer-260905-acp-priority-ports.md), [messages/huddle](implementer-260905-desktop-message-notification-huddle-ports.md), [projects](260905-projects-release-integration.md), [supporting sources](implementer-260905-supporting-source-ports.md), [renderer review](reviewer-260905-renderer-integration.md), and [final integration review](reviewer-260905-final-integration-fixes.md).

Final local evidence: `just ci` completed with exit 0 in `/tmp/crew-upgrade-just-ci-final-2.log`, including checks/builds, desktop 6991 passed with one skip, Tauri 3431 passed with 19 ignored plus other workspace targets, and mobile 2086 passed. The later browser-reconciliation desktop rerun passed 7012 tests with one skip and zero failures (`/tmp/crew-final-js-gate-pool.log`); final source-freeze checks (all guards), TypeScript and diff checks passed. `cargo-deny` reports advisories/bans/licenses/sources all OK in `/tmp/crew-upgrade-cargo-deny.log`. [Full integration](tester-260905-full-integration-gate.md) passed 126 DB, 379 PostgreSQL and 314 workspace integration tests; metrics attribution follow-ups passed too. Counts overlap by lane.

The filesystem snapshot was rechecked after the frozen membership-guard changes; classifications remain unchanged. It was previously refreshed after browser reconciliation. Eight previously target-exact paths now carry reviewed adaptations: agent config controls, project panel controls, and six browser specs. The composer right-click formatting commit (#6683) moves from ported to adapted because its browser test now matches the integrated composer. Dispositions are 24/155/12 with zero pending; there are no unexplained missing target paths.

[PR refresh/provenance/narrow-width review](debugger-260905-pr-hub-comment-refresh-loop.md) and [wheel retry review](reviewer-260905-timeline-wheel-retry.md) record the browser-discovered integration corrections. Final clean-bundle release smoke passed 3/3: exact 10,000-id hash, 199 continuations, maximum 164 mounted rows, no duplicate/order/render-pending errors; see [browser report](tester-260905-desktop-release-browser-evidence.md). [PR #342](https://github.com/Nuncio-hq/crew/pull/342) is open. The [NuncioCrew Gate job](https://github.com/Nuncio-hq/crew/actions/runs/33957899436/job/101285891754) and [manual sync workflow](https://github.com/Nuncio-hq/crew/actions/runs/33957896571) passed on `7b58b8d2d454e5506332f093946689db0247e99d`; the encompassing CI run was cancelled after browser failures. Subsequent browser repairs still require checks on their new head. No merge/release/hosted acceptance is claimed.

Later browser repairs are documented in [messaging](debugger-260905-messaging-preview-ci-reconciliation.md), [agent integration](debugger-260905-agent-integration-ci-reconciliation.md), [navigation/contrast review](reviewer-260905-pending-navigation-and-contrast.md), and [profile/config reconciliation](debugger-260905-profile-config-ci-reconciliation.md).

Final local follow-up: desktop 7012 passed + one existing skip (7013 total, 98.6s); huddle 24/24; real-relay browser 17/17 (22.4s). Full Projects 8/8 passed (23.5s); membership selection 13 and mounted guard four passed. New-head remote checks remain pending.

Docs impact: minor integration documentation and restored released operational/protocol docs. No external settings, personal configuration, commits, index changes or pushes were performed by this audit.

Unresolved source coverage questions: none. Final acceptance gates remain with the coordinator.
