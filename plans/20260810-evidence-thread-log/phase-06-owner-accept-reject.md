---
phase: 06
title: Owner Accept/Reject via NIP-25 reactions
status: planned
priority: P0
effort: M
dependencies: ["05"]
---

# Phase 06 — Owner Accept/Reject via existing reactions

Turns C3 green. Delivers the send/persist/render half of DoD checkbox 4; the
agent-readable half is verified in phase 09 against already-shipped CLI.

## Seams — this pattern already ships

The agent-receipt card implements exactly this interaction today:

| Behavior | Seam |
| --- | --- |
| Accept sends a kind-7 reaction | `desktop/src/features/messages/ui/MessageRow.tsx:408` — `onReviewed={() => handleReactionSelect("✅")}` |
| Reject carries a reason | `MessageRow.tsx:407` — `onRequestChanges={onReply ? () => onReply(message) : undefined}` opens the reply composer |
| Reaction state read back | `AgentReceiptMessageBody.tsx:45-50` — `reactions.some(r => r.emoji === "✅" && r.reactedByCurrentUser === true)` |
| Owner gating | `AgentReceiptMessageBody.tsx` — `profiles[normalizePubkey(message.pubkey)]?.ownerPubkey === currentPubkey` |
| Reaction transport | `useReactionHandler.ts` + `MessageReactions.tsx` — **upstream-owned, unchanged** |
| Agent reads the verdict | `buzz reactions get --event <id>` — `crates/buzz-cli/src/lib.rs:746-774`, **already exists** |

**No new event kinds, no new semantics, no workflow machinery** — the issue is
explicit about this, and the code agrees.

## Files

| File | Owner | Change |
| --- | --- | --- |
| `desktop/src/features/messages/ui/EvidenceCard.tsx` | Crew | Accept/Reject controls + verdict state |
| `desktop/src/features/messages/ui/MessageRowDefaultBody.tsx` | Crew | accept and forward `reactions`, `canToggleReactions`, `currentPubkey`, `reactionPending`, `profiles`, `onReply` |
| `desktop/src/features/messages/ui/MessageRow.tsx` | upstream-derived | prop pass-through only — **counts against the same ≤8-line budget as phase 05** |
| `desktop/src/features/messages/ui/useReactionHandler.ts` | upstream | **no change** |

## Behavior

1. **Accept** → kind-7 `✅` on the evidence event, through the same handler the
   receipt card uses. The card then shows an accepted state.
2. **Reject** → kind-7 `❌` **and** open the reply composer, so rejection carries
   a reason (RT-7). A bare ❌ with no explanation leaves the agent nothing to act
   on.
3. **Owner only.** Non-owners see the card and any existing reaction counts, but
   no Accept/Reject controls. Owner resolution reuses the receipt-card rule
   above; do not invent a second owner concept.
4. **Verdict state** is derived from the owner's own reaction on the event — it
   is not stored anywhere new. Reactions are durable owner-signed room events;
   that is the whole persistence story.
5. **No automation.** Nothing triggers on ❌. Any follow-up agent behavior is
   prompt-level (phase 02), per the issue.

## Open founder decision (D-2)

✅ already means "reviewed" on `KIND_AGENT_RECEIPT`. This phase reuses ✅ as
"accept" on evidence messages, per the issue's "no new semantics". They never
collide on one event, but it is one glyph with two nearby meanings. Implement the
default; if the founder picks a distinct pair, only the emoji constants and the
C3 assertions change.

## Acceptance criteria

- All C3 contract tests green.
- No edits to `useReactionHandler.ts` or `MessageReactions.tsx`.
- No new event kind; `crates/buzz-core/src/kind.rs` unchanged.
- Non-owner view has no Accept/Reject affordance.
- Reject opens the composer with the evidence message as parent.
- Reactions on ordinary messages behave exactly as today.

## Validation

```bash
cd desktop && pnpm test:e2e:smoke
just ci
```

Round-trip against a local relay is covered in phase 09.

## Anti-drift

Update `docs/crew/STATE.md` in the same PR (#117).

## Risk

Medium. The interaction is precedented, so the risk is duplication rather than
novelty: resist writing a second owner-resolution helper or a second reaction
sender. Rollback removes the controls; already-published ✅/❌ reactions remain
valid room events and still render as ordinary reactions.
