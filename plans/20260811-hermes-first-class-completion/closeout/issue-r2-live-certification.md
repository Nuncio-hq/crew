Title: Certify real Hermes Needs You and Project-thread operations

## Problem

The ACP elicitation and Project worktree mechanisms are present, but the
verification index has only records through 0006 and 0006 certifies Hermes
Slice 1 spawn/model behavior, not elicitation or Project-thread coding
(`docs/crew/verification/README.md:1-10`;
`docs/crew/verification/0006-hermes-slice1-live-roundtrip.md:8-13,21-24`).
The generic Project substrate is documented on main, but Hermes-specific
certification is still absent (`docs/crew/STATE.md:118-127`;
`crates/buzz-acp/src/thread_workspace.rs:29-61`).

## What to solve

Implement the verification-only R2 from
[`phase-02-live-certification.md`](../phase-02-live-certification.md):

- Run spike **S-D** to find a reproducible prompt/skill that makes real
  `hermes -p <profile> acp` emit `elicitation/create`.
- Produce exactly two records:
  `docs/crew/verification/0010-hermes-needs-you-live-roundtrip.md` and
  `docs/crew/verification/0011-hermes-project-runner-live.md`.
- Record A must cover single-select, multi-select, free text, same-turn
  resumption, decline, non-owner refusal, pending reconnect, and
  `Needs you` versus `Working`.
- Record B must cover worktree cwd, source-checkout isolation, retry branch
  reuse, restart reattach, stop release, and result disposition.
- Use an isolated relay/profile and mandatory
  `BUZZ_ACP_MCP_COMMAND=buzz-dev-mcp`; publish no secrets in evidence
  (`docs/crew/verification/0006-hermes-slice1-live-roundtrip.md:21-24,56-76`).

This issue produces **two verification records and no production code**.

## Definition of Done

- [ ] Spike S-D is PASS with a reproducible real-Hermes elicitation trigger.
- [ ] Both records exercise `hermes -p <profile> acp`, not a fake agent.
- [ ] Record A covers every listed form, ownership, reconnect, and status case.
- [ ] Record B covers cwd isolation, retry, restart, stop, and result disposition.
- [ ] Every FAIL becomes a bug issue with redacted evidence, never a workaround.
- [ ] No secrets appear in fixtures, logs, or screenshots.

## Evidence required

- Isolated relay, disposable profile, exact commands, event sequence, cleanup,
  and environment limits.
- Sanitized screenshots and logs showing the observed UI state.
- Explicit branch/commit/PR result facts or an explicit no-code result.
- The record must identify the real Hermes version and spawn command.

## Non-goals

No production code; no fake-agent-only certification; no provider credential
setup; no auth probing; no new ACP protocol; no Project-runner implementation;
no question-card redesign; and no secret-bearing fixture or screenshot.

## Dependencies

- PR #126 must merge first because its channel question card is the UI under test.
- Spike S-D gates the entire elicitation record.
- The existing generic substrate is evidence, not a substitute for Hermes proof.
