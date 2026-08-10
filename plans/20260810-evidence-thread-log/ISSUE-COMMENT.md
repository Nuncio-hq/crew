# Draft issue comment for #121 (not posted)

## Plan ready — 9 phases

Plan: `plans/20260810-evidence-thread-log/plan.md`. Planning only; nothing
implemented, nothing pushed. All 6 DoD checkboxes map to phases.

| # | Phase | Effort | Depends |
|---|---|---|---|
| 01 | Spike — unknown `crew-evidence` tag round-trip | S | — |
| 02 | Office prompt rule + thin-fork accounting | M | 01 |
| 03 | RED contract tests (CLI, desktop, reactions) | M | 01 |
| 04 | CLI `--evidence <kind>` | S | 03 |
| 05 | Desktop evidence card (four kinds) | L | 03 |
| 06 | Owner Accept/Reject via NIP-25 reactions | M | 05 |
| 07 | Upstream generic half — **blocked** | S | 02 |
| 08 | DECISIONS.md schema + limit, STATE.md | S | 04,05,06 |
| 09 | Live probes + Playwright evidence | M | 04,05,06,08 |

## Key design decisions

- **No `buzz-sdk` edit.** Tag appended to the built `EventBuilder` before
  signing, so `builders.rs` and `kind.rs` keep zero additional Crew delta.
- **Upstream footprint: 5 files, ~46 lines.** `base_prompt.md` and `messages.rs`
  have zero Crew delta today; both first edits justified and accounted.
- **`MessageRow.tsx` budget ≤8 lines** — it sits at 980/1000 against the ratchet.
  D-022 forbids raising `MAX_LINES`; logic lives in Crew-owned files.
- **No new CLI for reading verdicts** — `buzz reactions get --event` already
  ships, so DoD 4's agent-readable half needs verification, not code.
- **Prompt section capped at 18 lines, test-enforced** — the base prompt is paid
  on every turn of every agent; a fat section breaks the issue's own frugality rule.
- **kind 46043 ignores `crew-evidence`** — the receipt card keeps its own ✅, so
  no message shows two competing review affordances.
- **Generic-ACP (D-025): no Hermes-only behavior.** Honest limit: only the CLI
  can emit the tag this slice, not the desktop composer or mobile.

## Buzz seams named

`base_prompt.md:46` (office rule) · `buzz-acp/src/lib.rs:1942` (prompt reaches
every engine) · `buzz-sdk/src/builders.rs:250` `FAILURE_NOTICE_TAG` (custom tag
on kind 9 precedent) · `buzz-cli/src/client.rs:590` (post-build tag append) ·
`buzz-cli/src/lib.rs:398` + `commands/messages.rs:574` (flag surface) ·
`formatTimelineMessages.ts:520` → `types.ts:50` (tag reaches renderer, zero
plumbing) · `MessageRow.tsx:368/415` (dispatch) · `MessageRow.tsx:407-408` +
`AgentReceiptMessageBody.tsx:45-50` (accept via reaction, already shipping) ·
`buzz-cli/src/lib.rs:746-774` (`reactions get`) · `e2eBridge.ts:1153` (injection).

## Open questions

1. **Upstream PR conflict (blocks phase 07).** DoD 1 asks for a PR to
   `block/buzz`; D-020, root `AGENTS.md` and `UPSTREAM-SYNC.md:17` forbid it. The
   plan opens none — it writes a Crew-owned draft of the generic half. Keep
   D-020, or record a scoped exception?
2. **✅ overload.** ✅ already means "reviewed" on agent receipts and would also
   mean "accept" on evidence. Never collides on one event. Keep ✅/❌ as the issue
   specifies, or pick a distinct pair?
3. `UPSTREAM-SYNC.md` has **no upstream-file-edit list** — phase 02 creates one.

Validate: pass. Red-team: 9 findings, 8 applied, 1 escalated (question 1).
