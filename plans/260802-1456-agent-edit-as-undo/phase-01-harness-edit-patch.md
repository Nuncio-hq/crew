# Phase 1 — Harness: ingest edits, patch queued events, report outcome

**Crate:** `buzz-acp`
**Ships alone:** yes. Edits apply correctly; no UI claims anything yet.

## Context

The harness never sees message edits. In Mentions mode it subscribes to
`KIND_STREAM_MESSAGE`, `KIND_WORKFLOW_APPROVAL_REQUESTED`, `KIND_STREAM_REMINDER`,
`KIND_AGENT_USER_INPUT_ANSWER` (`src/lib.rs:1502-1509`). Kind 40003 is absent, so
editing a queued message changes the timeline and nothing else.

The queue already supports the shape we need: `remove_event(channel_id, event_id)`
walks both the normal queue and `withheld_native_steer` (`src/queue.rs:788-801`).
A patch is the same walk with a different mutation.

## Requirements

1. Subscribe to kind 40003 without letting it trigger a turn.
2. Patch the queued event's effective content when the target is still queued.
3. Report the outcome — patched vs too-late — as an observer frame.
4. Never mutate the signed `nostr::Event`.

## Files

| File | Change |
|---|---|
| `desktop/src/features/messages/lib/threading.ts` | `diffRemovedMentionPubkeys` — mirror of `diffAddedMentionPubkeys` (`:90-99`). |
| `desktop/src/features/messages/ui/MessageComposer.tsx` | Compute the removed set alongside the added set (`:545`) and pass it through. |
| `desktop/src-tauri/src/commands/messages.rs` | New `removed_mention_pubkeys: Option<Vec<String>>` param on `edit_message` (`:923-933`), mirroring `mention_pubkeys`. |
| `desktop/src-tauri/src/events.rs` | `build_message_edit` (`:411-428`) emits `["p-removed", <hex>]` per removed mention. |
| `desktop/src/shared/api/tauri.ts` | Thread the new arg through `editMessage` (`:617`). |
| `crates/buzz-acp/src/lib.rs` | Add 40003 to the Mentions default kinds (`:1502-1509`). New edit branch in the inbound loop, placed with the control-command branches (`:2142-2210`), i.e. **before** `filter::match_event` at `:2261`. |
| `crates/buzz-acp/src/queue.rs` | `QueuedEvent.edited_content` + `BatchEvent.edited_content`. New `patch_event()`. `format_event_block` prefers `edited_content` (`:1126-1160`). |

## Steps

### 1. Carry edited content beside the signed event

```rust
// queue.rs — QueuedEvent (:46) and BatchEvent (:56)
/// Replacement body from a kind:40003 edit that arrived before this event
/// was dispatched. The signed `event` is never mutated: its signature must
/// stay verifiable, and the relay is the authority on edit ownership.
pub edited_content: Option<String>,
```

`flush_next` already maps `QueuedEvent → BatchEvent` (`queue.rs:350-360`); carry
the field through. Every other `QueuedEvent`/`BatchEvent` construction site gets
`edited_content: None`.

### 2. Prefer the edited body when formatting the prompt

`format_event_block` (`queue.rs:1126-1160`) reads `be.event.content` in one place.
Change that read to `be.edited_content.as_deref().unwrap_or(&be.event.content)`.

Leave the `Tags:` dump alone — the tags are from the original signed event and
remain true.

Do **not** add "user edited this" framing. The agent never saw the original;
telling it about an edit invents a history that does not exist.

### 3. `EventQueue::patch_event`

Mirror `remove_event` (`queue.rs:788-801`) exactly, including the withheld set:

```rust
/// Replace the effective body of a still-queued event. Returns `false` when
/// the event is not queued — already dispatched, already cancelled-and-merged,
/// or never queued at all. Callers must treat `false` as "the agent has
/// already read the original" and report it, never as a silent success.
pub fn patch_event(&mut self, channel_id: Uuid, event_id: &str, content: String) -> bool
```

Deliberately **not** covered: `cancelled_batches`. Those events were already
delivered to the agent, so patching them would rewrite history the agent has
seen. Returns `false`, same as dispatched.

### 4. Inbound edit branch

Place with the other pre-filter branches (`lib.rs:2142-2210`), before the author
gate and `filter::match_event`. Placement is load-bearing: an edit event carries
no `p` tag unless the editor added a *new* mention (see the diffing comment at
`desktop/src/features/messages/hooks.ts:699-700`), so `require_mention` would drop
it if the branch sat after the filter.

```
if event.kind == KIND_STREAM_MESSAGE_EDIT:
    target = e-tag (64-hex)            # relay already validated shape+ownership
    removed = { t[1] for t in tags if t[0] == "p-removed" }

    if own_pubkey in removed:          # decision 1 — full undo
        acted = queue.remove_event(conversation_id, target)
        outcome = "dropped"
    else:
        acted = queue.patch_event(conversation_id, target, event.content)
        outcome = "patched"

    observer.emit("message_edit_applied",
                  { targetEventId, outcome, applied: acted })
    continue                            # never queue.push, never add a 👀
```

The drop decision reads **only** the `p-removed` tag set. No string matching on
the body — see the index's rationale; a false positive here silently discards a
real request, which is the one failure this feature must not have.

`remove_event` (`queue.rs:788-801`) currently returns `()`. Give it a `bool`
return (found-and-removed) so the outcome frame can be truthful, matching
`patch_event`. Its existing callers ignore the value.

Trust the relay on ownership and channel scope (`ingest.rs:782-822`) — it resolved
the target from the DB, which the harness cannot do. Do not re-derive; do assert
the `e` tag exists and is 64-hex before use.

`continue` is mandatory: an edit must not enqueue work, must not fire the
queue-accept 👀 (`lib.rs:2285-2300`), and must not extend any in-flight deadline.

### 5. Outcome frame

`message_edit_applied` rides the existing observer path (`context_for_conversation`,
same shape as `control_result` at `lib.rs:938-956`). Payload:

```json
{ "targetEventId": "<hex>", "outcome": "patched" | "dropped", "applied": true }
```

`applied: false` is the load-bearing case — it is what stops Phase 2 from lying.
`outcome` distinguishes "your edit rewrote the request" from "your edit cancelled
it", which are different enough that the UI must not merge them.

## Validation

Unit (`queue.rs` tests, alongside the existing `remove_event` coverage):
- patch hits an event in the normal queue → `true`, later `flush_next` batch
  carries the new body
- patch hits an event in `withheld_native_steer` → `true`
- patch on a dispatched / unknown / cancelled-batch id → `false`, no mutation
- patched event's `event.content` and signature are untouched
- prompt built from a patched batch contains the new body and not the original
- `p-removed` carrying this agent → event gone from the queue, no turn runs
- `p-removed` carrying a *different* agent → patched, not dropped
- edit whose body no longer names the agent but has no `p-removed` → patched,
  **not** dropped (guards the string-matching regression)

Desktop unit (`threading.ts` sibling test, alongside `diffAddedMentionPubkeys.test.mjs`):
- removed-set is the exact complement of the added-set over the same inputs
- self-pubkey never appears in either set

Integration — use the `acp-harness-e2e` skill (real process, fake ACP agent):
- queue an event behind a busy channel, publish a 40003 edit, release the turn,
  assert the agent's prompt has the edited body only
- publish the edit after dispatch, assert the prompt is unchanged and
  `message_edit_applied.applied == false` was emitted

Gate: `just ci`. `just test` as well — `buzz-relay` ingest is read but not
modified, so integration coverage is confirmation, not a requirement.

## Risk / rollback

Additive. Remove 40003 from the subscription list and the branch is unreachable;
`edited_content` stays `None` everywhere and `format_event_block` falls back to
existing behaviour.

Sharpest risk is misplacing the branch after `filter::match_event` — the edit is
then silently dropped for exactly the common case (typo fix, no new mention). The
integration test above is the guard.
