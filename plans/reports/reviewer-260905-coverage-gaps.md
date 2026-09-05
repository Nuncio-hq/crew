# Release coverage gaps — resolved 2026-09-05

**Status: DONE.** This replaces the intermediate missing-path snapshot. All discovered source gaps are imported or explicitly accounted for in the [final audit](reviewer-260905-coverage-audit.md) and [191-commit ledger](../260905-buzz-0-5-22-upgrade/coverage-ledger.csv). Local repository-wide and integration gates passed; final browser smoke passed and remote gates remain separate.

| Initial gap | Final resolution |
|---|---|
| Admin feedback filtering and Operator/Moderator auth UI | Released Admin sources imported; MIME fixtures follow actual sniffed image type, route test uses relay-status filter. Build/lint and 28 browser tests passed. |
| Benchmark evaluation corpus and cold-memory regression | Released corpus plus 68 required pre-pin dependencies imported. Benchmark 91 tests and testbed 39 tests passed; one live-provider skip. Declared dependencies validated in isolated public-PyPI environment because upstream lockfiles reference an unavailable private artifact registry. No frozen-lock validation claim. |
| Relay/push-gateway charts and deployment profiles | Released sources imported, except roster migration documentation maps target 0032 to Crew 0033. Helm 47 tests in nine suites, lint/render and push release-contract passed. No deployment performed. |
| Countdown example SDK call | Seventh SDK argument imported; cargo check passed. |
| Protocol and operator documentation | Fourteen missing/old released documents imported; agent availability prose adapted to Crew informational presence. Root architecture, contributor, test and agent guides merge released guidance with Crew identity and invariants. Pi credits added while retaining existing Crew runtime assets. |
| NIP-FI intermediate revisions | Final stateless release specification imported, including HTTP admission, deny-until-TTL and Git/Blossom exemptions; final ledger-removal migration mapped without rewriting applied Crew SQL. |
| Public APNS test fixture allowlist | Narrow released `.intersect/sadscan.yaml` rule imported; no broad secret-scanning bypass. |
| Protected Bestie E2E build | Released `desktop/.env.e2e` public feature flag restored; production opt-in remains unchanged. Browser verification is coordinator-owned. |
| Private reporting policy | Live GitHub setting is disabled. Crew policy and issue links now report that state without inventing a recipient or routing reports to Block. Maintainer setup remains explicit. |
| Upstream CI/security/staging lanes | Intentional Crew divergence: preserve NuncioCrew gate and manual release workflow; no new Block deployment or automated reviewer activated. |
| Renamed native accessor and migration files | Native accessors reuse `app-state-accessors.rs`; all twelve appended migration bodies verified byte-for-byte at Crew 0034–0045. |
| Session policy, zoom, theme, lifecycle hooks and file ceiling | Explicit Crew behavior retained; released features adapted around UUID/resume-first parallel threads, text-only zoom, informational presence, existing presentation and 1000-line guard. |

Verification details: [supporting-source report](implementer-260905-supporting-source-ports.md), [substrate report](260905-substrate-integration.md), [final integration review](reviewer-260905-final-integration-fixes.md), and [source audit](reviewer-260905-coverage-audit.md).

Unresolved source gaps: none. Local and final browser gates passed; remote acceptance, deployment and hosted-provider execution are not claimed.
