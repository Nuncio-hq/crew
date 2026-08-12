# Phase 02 — Live-Hermes certification (R2)

- **Status:** Proposed — not approved, not implemented
- **Issue:** #104 remainder / R2
- **Depends on:** PR #126 merged; spike S-D; no production code
- **Deliverable:** Two `docs/crew/verification/00NN-*.md` records
- **Boundary:** A real `hermes -p <profile> acp` process, isolated relay, disposable profile

## Deliverable

Produce two reproducible verification records, not implementation. Existing
main code has the durable elicitation substrate and same-turn continuation
(`crates/buzz-acp/src/elicitation.rs:1543-1617`,
`crates/buzz-acp/src/acp.rs:1586-1647`), while the verification index currently
holds records 0001–0006 only
(`docs/crew/verification/README.md:1-10`). Existing verification 0006 proves
a real Hermes relay round-trip and makes
`BUZZ_ACP_MCP_COMMAND=buzz-dev-mcp` mandatory
(`docs/crew/verification/0006-hermes-slice1-live-roundtrip.md:21-24,56-76`).
The requested real-Hermes Needs You certification is therefore new.

This phase must land after PR #126 (issue #110's channel question-card fix).
The question card is the UI under test; a passing protocol trace without the
fixed card is not certification.

## Gate 1 — spike S-D

Determine what reproducible prompt/skill makes a real Hermes emit
`elicitation/create`. Define the result before running:

| Result | Definition | Consequence |
| --- | --- | --- |
| PASS | A disposable profile and prompt reliably emit a form with a stable request ID | Proceed to RED and record A |
| FAIL | Hermes v0.20.x cannot emit the request through supported configuration | Do not fake the flow; file the Hermes-side bug/ask and stop record A |
| INCONCLUSIVE | Emission depends on an unrepeatable provider/tool condition | Narrow the spike or ask the founder; do not claim certification |

## Shared setup

| Item | Exact requirement |
| --- | --- |
| Relay | Disposable local Postgres/Redis/relay instance with isolated ports and no production data |
| Identity | Disposable owner and agent keys; owner is the only answer authority |
| Profile | Disposable named Hermes profile under isolated `HERMES_HOME`; no real credentials |
| Process | `hermes -p <profile> acp` spawned by the real Crew path |
| MCP | `BUZZ_ACP_MCP_COMMAND=buzz-dev-mcp` points to the built binary; mandatory per 0006 |
| Workspace | Disposable repository/worktree fixture, never the source checkout |
| Evidence | Redacted logs, screenshots, event IDs, commit/branch facts; no secrets |
| Cleanup | Stop process, release relay, remove profile/worktree/keys, retain only sanitized record |

## Record A — Needs You round-trip

Planned filename: `docs/crew/verification/0010-hermes-needs-you-live-roundtrip.md`.
Use one real Hermes session and a reproducible S-D prompt to exercise:

| Case | Setup | Observable PASS | FAIL becomes |
| --- | --- | --- | --- |
| Single-select | Form with one `oneOf` question | Card renders options; owner answer returns to same turn | Bug issue with trace and redacted screenshot |
| Multi-select | Form with array `anyOf` options | Multiple choices persist and same turn continues | Bug issue; no fixture workaround |
| Free text | Form with string question | Text answer is accepted and same turn continues | Bug issue; no alternate protocol |
| Decline | Owner declines | Terminal decline/cancel event; no stale Needs You | Bug issue with event sequence |
| Non-owner | Stranger answers the pending request | Answer rejected; request remains owner-visible | Security bug issue |
| Pending reconnect | Disconnect/restart while request is pending | Pending request returns exactly once and remains answerable, if S-D flow supports it | Durability bug issue; distinguish from existing orphan-cancellation evidence |
| Status | Pending request while agent is active | UI says `Needs you`, not Working/Permission/Failed | Projection/UI bug issue |

Main's current code supports normalization and same-loop continuation
(`crates/buzz-acp/src/elicitation.rs:1543-1617`,
`crates/buzz-acp/src/acp.rs:1586-1647`), terminal outcomes
(`crates/buzz-acp/src/acp.rs:1595-1633`), owner checks
(`crates/buzz-acp/src/elicitation.rs:1426-1457`), and status projection
(`desktop/src/features/agents/agentAttention.ts:30-40,188-192`). The record
must prove those contracts with Hermes rather than treating unit fixtures as
live evidence.

## Record B — Project-thread coding task

Planned filename: `docs/crew/verification/0011-hermes-project-runner-live.md`.
Use a deterministic local Git fixture and a Project thread:

| Case | Observable PASS | FAIL becomes |
| --- | --- | --- |
| Cwd | `session/new.cwd` is the disposable worktree | Bug issue; never change the source checkout |
| Write isolation | Hermes writes only inside the worktree | Safety bug issue with filesystem diff |
| Retry | Retry reuses the same branch/worktree, not a second one | Lifecycle bug issue |
| Restart | Restart reattaches the existing clean worktree | Recovery bug issue |
| Stop | Stop/cancel releases active-turn/worktree state | Cleanup bug issue |
| Result | Record branch/commit facts or explicit no-code result | Evidence-surface bug issue; no manual annotation workaround |

The substrate is engine-generic and already documented
(`docs/crew/STATE.md:118-127` on main; `crates/buzz-acp/src/thread_workspace.rs:29-61`
on main), while Hermes-specific certification is absent. PR #128 adds durable
evidence event kinds (`crates/buzz-cli/src/commands/evidence.rs:1-64`,
PR #128 / branch `devin/1786360062-evidence-thread-log`) but not a
Project-runner result projection; the base prompt only requires linking
Buzz-hosted results (`crates/buzz-acp/src/base_prompt.md:32-34` on main).
This record must not silently convert a generic evidence card into Hermes
proof.

## Implementation after approval

There is no production implementation in R2. After S-D is PASS and the RED
record templates are reviewed, implement only the two verification documents
under `docs/crew/verification/`. The records must preserve the real boundary,
commands, evidence, limits, cleanup, and result vocabulary; a failed
observation becomes a bug issue rather than a test-only workaround.

## Record format and verification commands

Each record must state boundary, exact commands, observed events, environment
limits, cleanup, and result, matching
[`verification/README.md`](../../docs/crew/verification/README.md).

```bash
. ./bin/activate-hermit
just relay
cargo build -p buzz-acp -p buzz-dev-mcp
cd desktop && pnpm exec playwright test tests/e2e/mission-inbox.spec.ts
just desktop-typecheck
git diff --check
```

Do not publish secrets, provider tokens, raw profile files, unredacted ACP
payloads, or screenshots containing private text.

## Exit criteria

- [ ] PR #126 is merged and the fixed channel question card is under test.
- [ ] S-D is PASS and its reproducible prompt is recorded.
- [ ] Both records exercise a real `hermes -p <profile> acp` process.
- [ ] Record A covers every listed question, ownership, reconnect, and status case.
- [ ] Record B proves cwd isolation, retry, restart, stop, and result disposition.
- [ ] FAILs become bug issues with evidence, never undocumented workarounds.

## Out of scope

Production changes; fake/buzz-agent-only certification; provider credential
setup; auth probing; new ACP protocol; Project-runner implementation; changing
the question-card UI in this phase; and any secret-bearing fixture or screenshot.
