---
phase: 03
title: Review in the channel
status: draft
priority: P0
effort: M
dependencies: ["02", "#121 / PR #128"]
---

# Slice 03 — Review in the channel

This slice is not started and is gated on receipt availability from Slice 00
and the evidence/acceptance seam in PR #128.

## Question and decision

Can the Mission strip derive review and completion from durable receipt and
reaction events without adding a second accept affordance? The answer must
reuse kind 46043 and the existing owner ✅ path.

## Verified seams

* Receipt publication is gated and outboxed at
  `crates/buzz-acp/src/pool.rs:5544-5589`.
* Receipt validation/projection is at
  `desktop/src/features/agents/agentReceiptStore.ts:107-244`.
* Receipt rendering and owner review behavior are at
  `desktop/src/features/messages/ui/AgentReceiptMessageBody.tsx:13-65` and
  `desktop/src/features/messages/ui/MessageRow.tsx:400-409`.
* Reactions are NIP-25 kind 7 (`crates/buzz-core/src/kind.rs:56-60`).
* Mission projection phase derivation is at
  `desktop/src/features/messages/lib/projectThreadMissionControl.ts:30-91`.

## RED contracts

* `receipt_projects_ready_for_review` — a valid unaccepted 46043 produces
  `ready_for_review`; it fails because Mission state has no receipt reducer.
* `owner_accept_projects_completed` — owner ✅ on the newest receipt produces
  `completed`; it fails because the Mission strip does not consume receipt
  review.
* `non_owner_reaction_does_not_complete` — another identity's reaction cannot
  complete the Mission; it fails until owner validation is connected.
* `receipt_history_is_preserved` — a later receipt supersedes the displayed
  state without deleting the earlier durable receipt; it fails because no
  Mission receipt selector exists.
* `receipt_state_survives_restart` — receipt then restart still shows review;
  accept then restart still shows completed; it fails until the projection is
  reconstructed from relay events.

## Implementation steps

1. Add durable receipt/reaction fixtures and restart tests.
2. Read the existing receipt projection; do not duplicate receipt parsing or
   reaction target validation.
3. Add `ready_for_review` and `completed` reads to the Mission strip.
4. Reuse existing owner ✅ behavior; do not add a second accept action.

## Gate — observable founder outcome

The founder sees a receipt in the thread, quits and relaunches, sees
`ready_for_review`, accepts it with the existing ✅ reaction, quits and
relaunches again, and sees `completed`.

## Risks and rollback

Receipts are disabled by default (`crates/buzz-acp/src/config.rs:1495`), so
Slice 00 must establish the real configuration first. A rollback removes only
the Mission read path; receipts and reactions remain durable and readable.

## Definition of Done

Receipt and reaction RED contracts are green; no duplicate review affordance is
introduced; history remains available; the restart gate passes.
