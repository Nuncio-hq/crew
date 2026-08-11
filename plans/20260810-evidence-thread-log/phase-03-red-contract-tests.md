---
phase: 03
title: RED contract tests (CLI, desktop, reactions)
status: planned
priority: P0
effort: M
dependencies: ["01"]
---

# Phase 03 — RED contract tests

Gate 3 of `docs/crew/DEVELOPMENT-WORKFLOW.md`. Every test in this phase must
**fail for the right reason** before phases 04-06 begin. The issue names this
explicitly: *"Contract tests RED first."*

## Contracts to pin

### C1 — CLI tag emission (Rust)

Location: `crates/buzz-cli/src/commands/messages.rs` test module (or a Crew-owned
sibling module if the upstream file's test section is contested).

- `--evidence test-run` on `messages send` produces exactly one
  `["crew-evidence","test-run"]` tag on the built event.
- All four kinds accepted: `test-run`, `metrics`, `before-after-visual`,
  `diff-stat`.
- An unknown value is rejected with the input-error exit code (1), and **no
  event is published**.
- Omitting `--evidence` produces a tag array byte-identical to today's.
- `--evidence` composes with `--file` (imeta tags both present) and with
  `--reply-to`.

### C2 — Desktop card render (Playwright, mock bridge)

Location: `desktop/tests/e2e/`, registered in `desktop/playwright.config.ts`
(`smoke` `testMatch` at `:30`). Inject tagged messages through
`__BUZZ_E2E_EMIT_MOCK_MESSAGE__` with `extraTags`
(`desktop/src/testing/e2eBridge.ts:1153`).

- Each of the four kinds renders its card (`data-testid` per kind).
- `metrics`, `test-run` and `diff-stat` are **fully legible with no images
  loaded** — assert the text content, not just card presence. This is the
  phone-friendly requirement from the issue.
- An unrecognized `crew-evidence` value renders the ordinary message body, not a
  broken card or an error boundary.
- A message with **no** `crew-evidence` tag renders exactly as today
  (non-regression).
- **R-2 contract:** a `KIND_AGENT_RECEIPT` (46043) message that also carries a
  `crew-evidence` tag keeps the receipt card and does **not** grow a second
  evidence card.

### C3 — Reaction round-trip on an evidence message

- Owner Accept sends a kind-7 `✅` targeting the evidence event id.
- Owner Reject sends a kind-7 `❌` and opens the reply composer.
- The card reflects the owner's own reaction after it lands.
- A non-owner viewer sees the card **without** Accept/Reject controls
  (owner resolution mirrors `AgentReceiptMessageBody`:
  `profiles[message.pubkey].ownerPubkey === currentPubkey`).
- Reactions on non-evidence messages are unaffected.

## RED validity criteria

Each test must fail because the behavior is absent, not because of a typo, a
missing import, or an unbuilt fixture. Record the observed failure message for
each contract in the PR description. A test that fails with
`Cannot read properties of undefined (reading 'invoke')` is a **build mistake**,
not a RED — that means `pnpm build:e2e` was skipped (root `AGENTS.md`).

## Steps

1. Write C1 first — it is cheapest and pins the wire format the other two assume.
2. Write C2 with `locator.screenshot()` scoping already in place, so phase 09's
   `shasum` distinct-state gate has a chance of passing (R-6).
3. Write C3 against the mock bridge's reaction path.
4. Run each suite, capture the failure output, confirm every failure is a
   genuine absence.

## Validation

```bash
cargo test -p buzz-cli evidence
cd desktop && pnpm test:e2e:smoke
```

## Acceptance criteria

- All C1/C2/C3 tests exist and are RED.
- Failure reasons recorded and each one is an absence, not an accident.
- No production behavior changed in this phase.

## Risk

Low, but this is the phase most likely to be skipped under time pressure.
Skipping it breaks the Crew workflow gate (AGENT-WORKING-AGREEMENT MUST NOT #8)
and removes the only proof that phases 04-06 actually delivered.
