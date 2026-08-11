---
phase: 01
title: Promote in place and reconstruct after restart
status: draft
priority: P0
effort: L
dependencies: ["00"]
---

# Slice 01 — Promote in place

This is the minimum shippable slice. It is not started. It does not add
execution, worktree, Inbox, or Projects behavior.

**Scope note — projection vs UI.** The pure projection is written *complete*
here (every state in the lifecycle, so the model is proven by fixtures before
any UI leans on it), but the only surfaces this slice ships are the promote
action and a strip that renders `planned`. Live `working`/`needs_input` is
Slice 02, review states are Slice 03, and the cancel/reopen **actions** are
Slice 04 — their projection contracts land here on purpose so a later slice
cannot quietly redefine the state model.

## Question and decision

How can the thread carry an explicit durable Mission fact while the visible
surface remains a projection over existing relay events? Settled D-1 = option A:
the marker is a kind-9 message with normal `h` and NIP-10 `e` tags plus:

```text
["crew-mission", "promote"]
["crew-mission-goal", "<short title>"]
```

This is the settled wire shape: no new event kind and no alternative marker
shape may be implemented in this slice.

Promotion is a manual human toggle. Never infer a Mission from a worktree,
receipt, ACP telemetry, or any other side effect; the valid marker is the only
Mission authority.

## Pure projection contract

Implement `missionState(threadRoot)` as a pure projection. It accepts:

* the verified root event;
* the complete set of durable events for that root, including ordinary
  messages, 46040/46042 input transitions, 46043 receipts, kind-7 reactions,
  and Git status events where applicable;
* the current authenticated owner pubkey;
* optional live active-turn input only as a display hint.

It returns either `null` for an unpromoted root or a value containing the
root ID, promotion event ID, goal/title, and one derived state from:
`planned`, `working`, `needs_input`, `ready_for_review`, `completed`,
`failed`, `cancelled`, or `reopened`.

The durable-state priority is: unresolved `46040` → `needs_input`; live
active-turn hint → `working` (never persisted truth); unaccepted 46043 →
`ready_for_review`; owner ✅ kind-7 on the newest receipt → `completed`;
failure receipt/Git status → `failed`; valid cancel/reopen transition;
otherwise valid promotion → `planned`. Existing durable kinds and receipt
validation are at `crates/buzz-core/src/kind.rs:583-599,632-640`,
`crates/buzz-acp/src/elicitation.rs:853-956,1027-1046`, and
`desktop/src/features/agents/agentReceiptStore.ts:182-244`.

The function must not read or write `conversationOutcomeLedger.ts`,
`needsYouStore.ts`, or `activeAgentTurnsStore.ts` as authority. It must
reconstruct with all three empty. Those stores are bounded/in-memory at
`desktop/src/features/agents/conversationOutcomeLedger.ts:11-51`,
`desktop/src/features/agents/needsYouStore.ts:12,35-50`, and
`desktop/src/features/agents/activeAgentTurnsStore.ts:83-106`.

## Validation and idempotency

Promotion events must pass the same strict relationship checks used for
receipts: valid owner author, exactly the expected `h`, canonical marker
bearing `e` ancestry to the selected root, and valid target/mention identity
where required. Mirror the checks at
`desktop/src/features/agents/agentReceiptStore.ts:107-175`; do not accept a
tag merely because it appears in an unrelated channel event.

The first valid owner-authored `promote` wins. Duplicate valid promotions do
not create another Mission. Arrival is deterministic by `(created_at, event
id)`, and a later-arriving older event cannot replace the winner. Unknown
`crew-mission` values are ignored. A `cancel` arriving before any valid
promotion is ignored in this slice.

## RED contract tests

Every test below is to be written before implementation and must fail for the
absence of the behavior, not because of a fixture or build mistake.

| Test name | Assertion | Why it fails today |
|---|---|---|
| `unpromoted_root_has_no_mission` | Ordinary thread with no marker returns `null`. | No Mission projection exists. |
| `valid_owner_promotion_projects_planned` | Valid owner marker with goal returns the root-scoped Mission and `planned`. | No parser/projection handles `crew-mission`. |
| `working_is_only_a_live_hint` | A live active-turn input may produce `working`, but the same durable fixture with that input removed does not. | No Mission projection separates observer telemetry from durable state. |
| `failed_receipt_or_git_status_projects_failed` | A failure receipt or allowed failure Git status produces `failed`. | No Mission projection consumes terminal failure facts. |
| `valid_cancel_projects_cancelled` | A valid owner cancel after promotion produces `cancelled`. | No Mission lifecycle marker projection exists. |
| `valid_reopen_projects_reopened` | A valid owner reopen after cancellation produces `reopened`. | No Mission lifecycle marker projection exists. |
| `ordinary_channel_mission_explains_no_worktree` | A promoted Mission outside a trusted Project workspace visibly says that it has no isolated worktree because the channel has no trusted Project workspace. | No Mission strip or plain-language worktree limitation exists. |
| `non_owner_promotion_is_rejected` | Same marker from a non-owner returns `null`. | No Mission-specific authorization path exists. |
| `wrong_channel_marker_is_rejected` | Mismatched `h` is ignored. | No strict Mission tag validator exists. |
| `wrong_root_or_reply_ancestry_is_rejected` | Unrelated root/reply `e` tags do not promote the selected root. | No Mission ancestry projection exists. |
| `unknown_mission_value_is_ignored` | `["crew-mission","unknown"]` renders as ordinary content and projects nothing. | No tolerant Mission tag parser exists. |
| `duplicate_promotions_are_idempotent` | Repeated valid promotions produce one winner. | No promotion identity or winner selection exists. |
| `out_of_order_promotions_choose_deterministically` | Input permutation produces the same `(created_at,event id)` winner. | No event-order resolution exists. |
| `cancel_before_promote_is_ignored` | A cancel without a prior valid promotion leaves the root unpromoted. | No cancel/reopen transition exists. |
| `mission_state_survives_empty_ephemeral_stores` | Replaying durable events with outcome, needs-you, and active-turn stores empty returns the same state. | Current state-bearing stores are local and no Mission projection exists. |
| `receipt_and_input_states_use_durable_events` | 46040/46042 and 46043/kind-7 fixtures derive state without local caches. | Existing UI selectors do not expose a Mission projection. |

Unit tests belong beside the projection as colocated `*.test.mjs`, following
`desktop/src/features/home/lib/missionInbox.test.mjs` and
`desktop/src/features/agents/agentReceiptStore.test.mjs`. The E2E seam is
`desktop/tests/e2e/` with `installMockBridge` at
`desktop/tests/helpers/bridge.ts:911-930`.

## UI and file ownership

Add a new Crew-owned thread Mission strip and promotion affordance. Do not add
the behavior directly to `desktop/src/features/messages/ui/MessageRow.tsx`:
the plan records that it is 980/1000 lines against
`desktop/scripts/check-file-sizes.mjs:8`, and D-022 forbids raising
`MAX_LINES`. Existing receipt dispatch is at
`desktop/src/features/messages/ui/MessageRow.tsx:400-409`; the new surface
must use a Crew-owned component seam instead.

The strip is thread-local, compact, and contains no cross-thread fields. For an
ordinary channel, it must plainly say that this Mission has no isolated
worktree because the channel has no trusted Project workspace; it must not fail
mysteriously. It does not create an Inbox/Projects surface, write a local
authoritative store, or infer promotion from a receipt/worktree. The limitation
is required by the fail-closed provisioning path at
`crates/buzz-acp/src/pool.rs:3019-3078`,
`crates/buzz-acp/src/thread_workspace.rs:244-270,141-148`.

## Implementation steps

1. Add the RED projection fixtures and strict tag-validation tests.
2. Implement the settled tolerant Mission marker parser and pure
   `missionState(threadRoot)` projection.
3. Add the owner-only promotion action using the existing message publish
   path (`crates/buzz-sdk/src/builders.rs:179-205,218-239`); preserve
   ordinary message rendering for clients that ignore the tag.
4. Add the new Crew-owned strip and action component without exceeding the
   `MessageRow.tsx` ratchet.
5. Add a restart/replay test that clears ephemeral stores before projection.

## Gate — observable founder outcome

The founder promotes a normal channel thread, quits the app, relaunches,
opens the same channel, and sees the Mission strip reconstructed from relay
events alone. No local cache is primed. If the strip disappears, Slice 01 has
not passed and later slices do not start.

## Risks

* The settled tag shape must remain the only accepted promotion wire shape.
* Weak `h`/`e` validation could let unrelated channel members promote another
  thread.
* Reading TTL-bound stores would make restart behavior appear correct only
  during the current process.
* Editing `MessageRow.tsx` beyond the permitted budget would violate D-022.

## Rollback

Remove the new projection and UI code. Published marker messages remain
ordinary readable messages to clients that do not understand the tag. No
migration or local database cleanup is required.

## Review checklist

* The Mission marker is the only authority, and promotion is a manual human
  toggle; no worktree, receipt, telemetry, or other side effect infers a
  Mission.
* Outside a trusted Project workspace, the strip plainly explains that no
  isolated worktree exists because the channel has no trusted Project workspace.
* If anyone requests Mission priority or ordering, stop and return to the
  founder for a fresh decision; do not settle it in implementation.

## Definition of Done mapping

* Explicit promotion produces one owner-authored durable marker.
* All RED tests are green after implementation.
* Mission reconstruction passes with ephemeral stores empty.
* The strip survives quit/relaunch.
* No file-size limit is raised and no non-Crew Mission authority is added.
* Any request for Mission priority or ordering stops the work and returns to
  the founder; it is not an implementer-settled detail.
