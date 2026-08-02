# Edit-as-Undo — steering v2

**Status:** proposed (not started)
**Depends on:** v1 (composer morph button + dispatch hold) — soft dependency, see below.

## Outcome

Editing a message the agent has **not yet read** rewrites what the agent will
read. Editing one it **has** read does not, and the UI says so instead of
silently doing nothing.

## Why this is not just cosmetic

The editable window is **not** the 2s hold. It is "until dispatch", and today
that is often much longer: a channel with an in-flight turn holds every new
event in the queue until the current turn completes
(`flush_next` skips in-flight channels — `crates/buzz-acp/src/queue.rs:306-315`).
So a message queued behind a 4-minute turn is editable for 4 minutes.

v1's hold only guarantees a *minimum* window. **v2 works without v1**; v1 makes
the window predictable for the common idle-agent case.

## Current state (verified)

| Fact | Anchor |
|---|---|
| Harness (Mentions mode) subscribes to 4 kinds; 40003 not among them | `crates/buzz-acp/src/lib.rs:1502-1509` |
| Edit event = kind 40003, `e` tag → target id, `h` tag → channel, content = new body | `crates/buzz-relay/src/handlers/ingest.rs:782-822` |
| Relay already enforces same-author + same-channel on edits | `ingest.rs:782-822` |
| Queue can already drop a queued event by id (queue + withheld) | `queue.rs:788-801` |
| Prompt body reads `event.content` verbatim | `queue.rs:1126-1160` |
| `turn_started` already carries `triggeringEventIds` | `crates/buzz-acp/src/pool.rs:1402-1416` |
| Desktop + mobile already parse `triggeringEventIds` | `desktop/src/features/agents/ui/agentSessionTranscript.ts:149-158`, `mobile/lib/features/channels/agent_activity/transcript_builder.dart:375` |
| Queue is a single `&mut` owned by the main loop — push and flush cannot interleave | `lib.rs:2285`, `lib.rs:3037-3065` (`dispatch_pending`) |

Consequence: the dispatch boundary is **already observable end-to-end**. No new
protocol is needed to know whether a message is still editable.

## Phases

| # | Phase | File |
|---|---|---|
| 1 | Protocol: removed-mention signal, ingest 40003, patch-or-drop, report outcome | [phase-01-harness-edit-patch.md](phase-01-harness-edit-patch.md) |
| 2 | Presentation: editable-window states on the message | [phase-02-desktop-editable-window.md](phase-02-desktop-editable-window.md) |

Phase 1 owns the whole protocol path including the desktop **emit** side (the
`p-removed` tag), so drop-on-mention-removal works end-to-end after Phase 1
alone. Phase 2 is presentation only.

Phase 1 ships alone and is independently correct (edits apply; no UI claims
anything). Phase 2 only adds the affordance. Do not ship Phase 2 first — an
"undo" label with no Phase 1 behind it is the exact silent-no-op failure this
plan exists to avoid.

## Acceptance criteria

1. Message edited while queued → agent's prompt contains **only** the final
   text. No trace of the original, no "user edited" framing (the agent never
   saw the original, so there is nothing to reconcile).
2. Message edited after its id appears in a `turn_started` → agent context
   unchanged, and the editor is told the edit did not reach the agent.
3. Editing a message never enqueues a new turn and never fires a second 👀.
4. Edit by a non-author is impossible (already enforced at the relay; assert it
   is not re-introduced at the harness).
5. Patch applies to both the normal queue and the withheld-steer set, matching
   `remove_event`'s coverage.
6. Cancelled-and-requeued batches are **not** patchable — the agent already read
   those events. Reported as too-late, same as the dispatched case.
7. Edit that removes this agent's mention, while the event is still queued →
   the event is dropped from the queue; no turn ever runs for it.
8. An edit carrying no `p-removed` for this agent never drops anything, however
   the body text changed. No string matching against the agent's own name
   anywhere in the harness.

## Non-goals

- Editing a message mid-turn to steer. That is the existing steer path (send a
  new message); do not overload edit with it.
- Retroactively rewriting agent context after dispatch.
- Edit support for kinds other than 40003 chat message edits.
- Mobile UI. Phase 1 makes mobile correct-but-silent; a mobile affordance is a
  separate follow-up once the desktop shape is proven.

## Risk

| Risk | Mitigation |
|---|---|
| Edit lands after flush (wall-clock race) | Unavoidable in principle. Harness reports the true outcome; desktop corrects its optimistic UI. Never silently succeed. |
| Mutating a signed event's content breaks integrity assumptions | Do not mutate. Carry `edited_content: Option<String>` alongside; prompt formatting prefers it. |
| Subscribing 40003 accidentally wakes the agent | Handle the edit branch **before** `filter::match_event` and never call `queue.push` for it — same placement discipline as the existing control-command branches (`lib.rs:2142-2210`). |
| Edit spam rewrites a queued event repeatedly | Last-write-wins is correct and matches chat semantics. No extra guard. |

## Rollback

Phase 1 is additive: drop 40003 from the subscription list and the edit branch
becomes dead. Phase 2 is a UI-only revert. Neither phase changes existing
dispatch behaviour for unedited messages.

## Decisions (Oscar, 2026-08-02)

1. **Edit that removes the agent mention → drop from the queue entirely.** Full
   undo, not a patch. Removing the mention is read as "I didn't mean to ask
   you."
2. **Too-late state is inline on the message**, not a toast. It is a correctness
   fact, not a transient notice.

### How "mention removed" is detected — explicit signal, not text matching

Decision 1 needs a signal that does not exist yet, and the obvious shortcut is
wrong.

`require_mention` matches on **`p` tags**, not body text
(`crates/buzz-acp/src/filter.rs:91-93`). A desktop edit emits `p` tags only for
mentions the edit **adds** (`desktop/src-tauri/src/commands/messages.rs:929-932`,
`desktop/src/features/messages/lib/threading.ts:90-99`) — removals are not
signalled at all, and the original signed event keeps its `p` tags forever. So
the harness cannot see a removal today.

**Do not infer removal by searching the edited body for the agent's `@Name` /
npub / hex.** Failure modes, in the destructive direction:

- Display names are mutable and non-unique; the harness would need a profile
  lookup to even know its own rendered name.
- Addressing is routed by `p` tag, not prose. A body that never spells the name
  ("làm giúp mình cái này") is still a real request — text absence is not intent.
- A body can keep `@Agent A` while the edit removed `@Agent B`; every agent
  string-matching its own name gets a different answer to the same question.
- A false positive **silently drops the user's request**. Wrong-direction
  failure: the safe error is "still runs", never "never runs".

Instead, the composer already computes the exact diff — it holds both the
original and edited mention sets at `MessageComposer.tsx:545`. Emit the removed
half explicitly:

- `threading.ts` gains `diffRemovedMentionPubkeys`, the mirror of the existing
  `diffAddedMentionPubkeys` (`threading.ts:90-99`; colocated test file already
  exists).
- The edit event carries removed mentions as their own tag, e.g.
  `["p-removed", "<hex>"]`, added in `build_message_edit`
  (`desktop/src-tauri/src/events.rs:411-428`) via a new
  `removed_mention_pubkeys` param on the `edit_message` command
  (`commands/messages.rs:923-933`), mirroring how `mention_pubkeys` already
  rides.
- Harness: own pubkey present in the removed set → `remove_event` (full undo).
  Otherwise → `patch_event`.

Relay impact: none expected. Kind 40003 skips the generic membership gate and
`validate_edit_ownership` is the authority (`ingest.rs:1866-1878`); that
validator requires only the `e` tag, same author, and same channel
(`ingest.rs:782-822`). Searched the 40003 ingest path in `ingest.rs` only —
confirm with a live publish before relying on it.
