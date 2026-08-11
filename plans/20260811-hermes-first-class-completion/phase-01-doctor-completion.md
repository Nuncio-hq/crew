# Phase 01 — Hermes Doctor completion (R1)

- **Status:** Proposed — not approved, not implemented
- **Issue:** #104 remainder / R1
- **Depends on:** PR #134 merged; spikes S-A and S-C
- **Contracts:** Doctor state, dependency probe, safe diagnostics, refresh
- **PR scope:** Crew-owned Rust/TS surfaces + focused E2E; no Hermes upstream edit

## Deliverable

Complete the Hermes Doctor delta on top of #134's profile-readiness evaluator.
The evaluator already exposes `Ready`, `Missing`, `BrokenConfig`,
`BinaryMissing`, and `AuthUnknown` (`desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:20-39`,
PR #134 / branch `agents/profile-lifecycle-hardening`), resolves and probes
the binary (`desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:59-80`,
PR #134 / branch `agents/profile-lifecycle-hardening`), and checks the bound
profile/config (`desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:87-149`,
PR #134 / branch `agents/profile-lifecycle-hardening`). This phase adds only
the missing compatibility/dependency/update diagnostics; it does not add an
auth probe or auth badge.

## Gate 1 — prerequisite spikes

Run S-A and S-C under
[`DEVELOPMENT-WORKFLOW.md`](../../docs/crew/DEVELOPMENT-WORKFLOW.md) Gate 1.
Define the result before running:

| Spike | Question | PASS | FAIL | INCONCLUSIVE |
| --- | --- | --- | --- | --- |
| S-A | Is `hermes --version` parseable and is `hermes acp --check` truthful per profile? | Stable version parse plus deterministic check exit/status for healthy, broken, and missing profiles | Output/exit semantics are unstable or check is dependency-only | Platform/version variation prevents a reproducible answer |
| S-C | Does `hermes update --check` exist and have no side effects? | Command exists, is read-only, and returns parseable update/no-update output | Missing command or any mutation/network install side effect | Output or side effects cannot be distinguished reliably |

S-A PASS is required for `Incompatible` and the ACP dependency probe. S-A
FAIL/INCONCLUSIVE means retain a truthful `NotVerified` result and drop those
checks. S-C FAIL/INCONCLUSIVE drops update discovery; it never becomes a
best-effort mutating update path.

## Design decisions

| ID | Topic | Choice |
| --- | --- | --- |
| 1 | State model | Preserve #134's `AuthUnknown`; add `Incompatible` only after S-A PASS |
| 2 | Profile check | Keep the existing bound-directory/config checks and their blocking behavior |
| 3 | ACP probe | Invoke `hermes acp --check` read-only with bounded timeout and sanitized output |
| 4 | MCP probe | Check the resolved `buzz-dev-mcp` executable, not a human-readable reply |
| 5 | Refresh | Add an explicit Hermes Doctor re-check action wired through existing IPC/query invalidation |
| 6 | Repair | Show a copyable command and official documentation link per actionable state |
| 7 | Updates | Add `hermes update --check` only after S-C PASS; never run update |
| 8 | Auth | No auth badge; no text scraping; `AuthUnknown` remains honest |

The MCP check is motivated by the documented real failure class: a missing
MCP path produces `auth error: BUZZ_PRIVATE_KEY is required`
(`docs/crew/HERMES.md:163-167` on main).

## RED contract table

Write these tests before implementation. Every RED must fail for the intended
missing behavior and observe a public state/presenter contract.

| ID | Scenario | Expected | Forbidden |
| --- | --- | --- | --- |
| DR-01 | Resolved binary is absent | `BinaryMissing`/actionable state with no spawn | Silent `Ready` |
| DR-02 | Version is below S-A minimum | `Incompatible` with observed/required versions | Starting the profile |
| DR-03 | Version output is malformed | `NotVerified`/unknown diagnostic | Guessing compatibility |
| DR-04 | `hermes acp --check` fails | Dependency failure is surfaced with bounded, sanitized detail | Treating exit 0 from another command as proof |
| DR-05 | `buzz-dev-mcp` is unavailable | Actionable MCP repair state and command | Reporting an auth failure as authentication truth |
| DR-06 | Bound profile directory/config is absent or invalid | Existing #134 missing/broken state remains blocking | Binding or starting anyway |
| DR-07 | Re-check after external repair | IPC/query refreshes state without app restart | Stale card after successful re-check |
| DR-08 | Actionable state displayed | Copyable repair command plus official link | Uncopyable or invented URL |
| DR-09 | `hermes update --check` says update available | Read-only update fact is shown | Installing, rewriting, or mutating profile state |
| DR-10 | Diagnostic contains paths, tokens, or secrets | Only allowlisted fields/classes cross the presenter boundary | Raw stdout/stderr in UI, logs, or relay |
| DR-11 | Auth cannot be probed truthfully | State is `AuthUnknown`; no auth badge | Inferring auth from version/check text |

## Likely touch set

| Area | Likely file | Purpose |
| --- | --- | --- |
| Rust evaluator | `desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs` | Version, ACP, MCP, update facts and sanitized state |
| Rust readiness | `desktop/src-tauri/src/managed_agents/readiness/hermes.rs` | Requirement mapping and blocking semantics |
| Rust command/IPC | `desktop/src-tauri/src/managed_agents/runtime_commands.rs` | Explicit re-check/invalidation seam |
| Presenter | `desktop/src/features/agents/hermesProfileReadinessPresenter.ts` | Stable labels, repair copy, allowlisted diagnostics |
| Presenter tests | `desktop/src/features/agents/hermesProfileReadinessPresenter.test.mjs` | RED state/copy/redaction contracts |
| Agent card | `desktop/src/features/agents/ui/AgentHarnessField.tsx` | Doctor state and refresh action surface |
| E2E | `desktop/tests/e2e/doctor-states.spec.ts` | Visible state, refresh, repair-copy behavior |
| E2E | `desktop/tests/e2e/doctor-cta-screenshots.spec.ts` | Screenshot-level actionable Doctor states |

These paths exist on #134; the evaluator is at
`desktop/src-tauri/src/managed_agents/hermes_profile_readiness.rs:87-149`
(PR #134 / branch `agents/profile-lifecycle-hardening`), and the existing
refresh action is in
`desktop/src/features/settings/ui/HarnessesSettingsPanel.tsx:90-133`
(PR #134 / branch `agents/profile-lifecycle-hardening`).

## Implementation after approval

After S-A/S-C are conclusive and DR-01…DR-11 are RED, implement the smallest
Crew-owned delta in the touch set. Preserve #134's state serialization and
generic readiness pipeline; do not rename `AuthUnknown` or introduce an auth
claim. If a spike is not PASS, remove that contract from the implementation
plan rather than substituting a heuristic.

## Verification

After approval and implementation:

```bash
. ./bin/activate-hermit
cargo test --manifest-path desktop/src-tauri/Cargo.toml
pnpm --filter buzz check
cd desktop && pnpm exec playwright test tests/e2e/doctor-states.spec.ts
cd desktop && pnpm exec playwright test tests/e2e/doctor-cta-screenshots.spec.ts
just desktop-typecheck
git diff --check
```

## Exit criteria

- [ ] S-A and S-C are conclusive and recorded before implementation.
- [ ] DR-01…DR-11 are RED first, then GREEN with focused tests.
- [ ] `Incompatible`, ACP dependency, MCP, refresh, update, and redaction
      behavior are observable without claiming auth truth.
- [ ] Auth remains `AuthUnknown`; no auth badge ships.
- [ ] `git diff --check`, focused tests, typecheck, and required desktop gates pass.

## Out of scope

Auth probing or an auth badge; Hermes upstream changes; profile export/import;
editing model, memory, skills, credentials, plugins, cron, gateways, or
webhooks; automatic update/install; raw diagnostics in relay/UI; remote or
public profile-bound agents; and any new Hermes-only protocol.
