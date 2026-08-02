# Phase 2 — `retry_turn` control frame

**Depends on:** [phase 1](phase-01-machine-readable-failure-notice.md) — the
desktop reads the failed event ids off the notice's tags.

## Context

Stop already established the shape: the desktop sends an observer control frame,
the harness acts on its queue and pool, and a `control_result` frame comes back
carrying a status the desktop renders distinctly
(`handle_cancel_turn_control`, `crates/buzz-acp/src/lib.rs:931`).

Retry is the same shape pointed the other way, with one difference: the failed
batch is gone. `requeue` dead-letters by returning the batch to the caller
(`crates/buzz-acp/src/queue.rs:513-527`), and the caller drops it after posting
the notice. So retry has to reconstruct the work from the relay —
`RestClient::query` (`crates/buzz-acp/src/relay.rs:399`) fetches events by id.

## Requirements

1. Desktop sends `retry_turn` with `channelId`, `conversationId`, and the failed
   event ids from the notice. It must **never** publish a fresh kind-9 carrying
   the same body — that duplicates the person's message, moves it to the bottom
   of the timeline, and re-notifies everyone it mentions.
2. Harness handler beside `handle_cancel_turn_control`: fetch the events, rebuild
   `QueuedEvent` with `hold_exempt: true` — matching that field's documented
   meaning, traffic that already waited (`queue.rs:56-58`) — and push.
3. The channel's retry budget is clear before the retry runs, so it gets a full
   allowance rather than dead-lettering on the first stumble. Dead-lettering
   already clears `retry_counts` and `retry_after` (`queue.rs:524-528`) — verify
   that rather than assuming it, since the auth path dead-letters *without*
   going through `requeue` (`lib.rs:3609-3622`) and so never touches either map.
4. `control_result` reports `"type": "retry_turn"` with distinct statuses, each
   rendered differently in the desktop exactly as the Stop toasts are. At
   minimum: dispatched, events no longer on the relay, turn already running.
5. Retry authorization mirrors Stop's, which needs no new desktop-side gate:
   the harness drops observer control frames from a non-owner outright
   (`lib.rs:869`) and drops them when no owner resolves (`lib.rs:2011`), so the
   control-frame transport is owner-scoped by construction. Retry being
   causative where Stop is conservative does not open a hole — neither is
   reachable by a non-owner. Consequence to keep in mind: a dropped frame
   produces no `control_result`, so the desktop's wait falls through to its
   unconfirmed timeout, exactly as Stop does today.

## Settled

1. **Edits must be re-resolved — retry may not run pre-edit text.** A
   pre-dispatch edit lives in queue memory (`queue.rs:52-55`); the signed event
   keeps its original body because the relay is the authority on edit ownership.
   A relay re-fetch by id therefore returns the *original* text. Auto-requeue
   preserves `edited_content`; a manual retry through the relay would not.

   Shipping that as a known limitation is not acceptable: edit-as-undo promises
   that the edit is what the agent sees, so a Retry that silently re-runs the
   replaced text does work the person explicitly cancelled. That is worse than a
   no-op, and it is the same lying-affordance standard that killed the phantom
   Stop button.

   `KIND_STREAM_MESSAGE_EDIT` (40003, `buzz-core/src/kind.rs:470`) is a regular
   stored kind — not ephemeral, not replaceable — so the latest edit per target
   is queryable in the same `RestClient::query` round trip the handler already
   makes. Apply it to the rebuilt `QueuedEvent.edited_content`.

   The edit references its target with a plain `["e", <target-id>]`
   (`lib.rs:2946-2959`), and `e` is a single-letter indexed tag, so
   `{kinds:[40003], "#e":[ids]}` is a real relay filter rather than a scan.

   Two consequences that fall out of re-resolving:

   - **Ownership needs no re-check.** The relay validates at ingest that a
     kind:40003's author matches the target's effective author
     (`buzz-relay/src/handlers/ingest.rs:780`), so anything read back is already
     authorship-validated. Do not add a redundant check.
   - **`p-removed` withholds its own event, per event.** An edit can carry
     `["p-removed", <agent-hex>]`, which is how edit-as-undo undoes a queued
     request (`edit_removes_agent`, `lib.rs:2966`). After dispatch that edit is
     a no-op today, so a failed event can carry a later edit that un-mentions
     this agent. Retry must honour it — re-running work the person took away
     from this agent is the same failure as running their replaced text, one
     step worse.

     Scope it per event, not per batch: the live path drops exactly the one
     target via `queue.remove_event_by_id` (`lib.rs:2997`) and lets the next
     flush rebuild the merged prompt from what remains. A retry that discarded
     all 50 events because one was un-mentioned would throw away 49 legitimate
     requests and contradict the semantics the same tag already has.

     So: drop the withheld events, retry the remainder, and reach the veto
     status only when nothing survives. Distinguish "all withheld" from "some
     withheld" — someone who clicked Retry on a multi-event notice needs to know
     part of it was held back.

   If re-resolving turns out to need more than that one added filter, disable
   Retry for events carrying edits rather than silently running stale text.

2. **Batch granularity: whole batch.** `flush_next` drains up to
   `MAX_BATCH_EVENTS` into one `FlushBatch` and `requeue` returns the whole
   thing, so the batch is the unit that failed and was reported. Rebuild one
   batch from all `failed` ids rather than one turn per id.

## Risk

Retry is a re-execution primitive: the failed turn may have completed real side
effects before dying (files written, PRs opened, messages sent). Nothing here
detects that, and a side-effect ledger is out of scope. Keep the copy honest
about re-running rather than implying resumption.

## Validation

- Unit: retry pushes the original signed event unmutated, with `hold_exempt` set.
- Unit: retry when the relay no longer holds the events reports its own status
  and pushes nothing.
- Unit: retry while a turn is already running for that conversation does not
  double-dispatch.
- Unit: the retry budget is clear for both the exhaustion path and the auth path.
- Desktop unit: each status renders distinctly; no path publishes a kind-9.
