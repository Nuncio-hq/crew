# Release coverage audit — 2026-09-05

**Status: DONE.** All 191 commits in the recorded upstream interval have a final source disposition. This is source coverage, not repository-wide acceptance, merge, release, deployment, or hosted-provider validation.

## Boundary and method

- Target `desktop-v0.5.22`: `9ceb1f79bbc21785a0a075c40aecb3c058b1ea15`.
- Previous recorded pin: `39f8b46935736334cdd7045a4e4b5d7eb1a33888`.
- Actual shared ancestor: `4749bc7be3cdb78c2db4ce4864775ba7ab60b4cc`.
- Pre-integration Crew HEAD: `871eecb18d7a243d87ec56a2eb154fbf2099d7ce`.
- Read each commit's touched-path inventory, group by final released source, and reconcile against subsystem implementation/review reports. Compare current filesystem bytes, including untracked additions, against target and Crew HEAD with `git cat-file --batch`. Comparing only `git diff` would incorrectly mark untracked imports as missing.
- The [ledger](../260905-buzz-0-5-22-upgrade/coverage-ledger.csv) contains 191 unique SHAs: **25 ported, 154 adapted, 12 retained-Crew-divergence, zero pending**. No row was labeled already-equivalent without whole-row proof.
- The [machine-readable source snapshot](260905-final-source-coverage.json) accounts for 1820 unique ledger paths: **1428 target-exact, 350 adapted, 42 unchanged Crew**. Target-exact includes paths deleted in both final target and Crew. Adapted is a byte comparison, not an independent correctness verdict; the ledger attaches subsystem evidence and concrete current paths.
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

Other deliberate adaptations: 1000-line guard and exact existing baselines; Crew theme, settings dropdowns, syntax treatment, runtime avatars, and 13/14/15 conversation font contract; Hermes ownership and Save/Cancel semantics; native macOS-only glass behavior; repository/outcome features on Crew's channel-first surface. Existing rich/compact link-preview settings remain functional through Crew presentation code; the upstream cosmetic preview helper is not wired into Crew settings.

Private vulnerability reporting was verified disabled for `Nuncio-hq/crew` through GitHub's read-only repository API. `SECURITY.md`, issue templates and contributor links now state the actual availability instead of sending Crew vulnerabilities to Block or inventing a private contact/SLA. Enabling private reporting or naming a private recipient remains maintainer setup, not an unresolved source port.

## Gaps found and resolved

The [resolved gap report](reviewer-260905-coverage-gaps.md) records missing Admin UI, chart, benchmark, documentation and example dependencies, subsequently imported and reviewed. Narrow public APNS fixture allowlisting was imported from target. Protected E2E restores the released `VITE_BUZZ_BESTIE=1` build flag; production default is unchanged.

Subsystem evidence is linked per ledger row. Key reports: [substrate](260905-substrate-integration.md), [native](implementer-260905-native-release-integration.md), [ACP](implementer-260905-acp-priority-ports.md), [messages/huddle](implementer-260905-desktop-message-notification-huddle-ports.md), [projects](260905-projects-release-integration.md), [supporting sources](implementer-260905-supporting-source-ports.md), [renderer review](reviewer-260905-renderer-integration.md), and [final integration review](reviewer-260905-final-integration-fixes.md).

Final local evidence: `just ci` completed with exit 0 in `/tmp/crew-upgrade-just-ci-final-2.log`, including checks/builds, desktop 6991 passed with one skip, Tauri 3431 passed with 19 ignored plus other workspace targets, and mobile 2086 passed. The final desktop rerun including the wheel regressions passed 6996 tests with one skip and zero failures (`/tmp/crew-upgrade-desktop-test-final.log`). `cargo-deny` reports advisories/bans/licenses/sources all OK in `/tmp/crew-upgrade-cargo-deny.log`. [Full integration](tester-260905-full-integration-gate.md) passed 126 DB, 379 PostgreSQL and 314 workspace integration tests; metrics attribution follow-ups passed too. Counts overlap by lane.

The filesystem snapshot was refreshed after the final metrics and presentation changes. Four previously target-exact paths now carry reviewed adaptations: `crates/buzz-db/src/store/event.rs`, Bestie `BloomMenu.tsx`, shared `theme.css`, and `bestie.spec.ts`. Commit disposition counts remain 25/154/12 with zero pending; the refresh introduced no unexplained missing target path.

[PR refresh/provenance/narrow-width review](debugger-260905-pr-hub-comment-refresh-loop.md) and [wheel retry review](reviewer-260905-timeline-wheel-retry.md) record the browser-discovered integration corrections. Final clean-bundle release smoke passed 3/3: exact 10,000-id hash, 199 continuations, maximum 164 mounted rows, no duplicate/order/render-pending errors; see [browser report](tester-260905-desktop-release-browser-evidence.md). Remote PR, NuncioCrew Gate and manual upstream-sync execution have not yet occurred; no merge/release/hosted acceptance is claimed.

Docs impact: minor integration documentation and restored released operational/protocol docs. No external settings, personal configuration, commits, index changes or pushes were performed by this audit.

Unresolved source coverage questions: none. Final acceptance gates remain with the coordinator.
