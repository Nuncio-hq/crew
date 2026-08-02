# Phase 1 — Harness: dispatch hold + cancel reaches queued work

**Crate:** `buzz-acp`
**Ships alone:** yes, but it is invisible without Phase 2. Land it first anyway —
Phase 2 wired to a no-op cancel is the failure mode this plan exists to avoid.

## Requirements

1. A human-authored event is not dispatched until it has sat in the queue for the
   hold duration. Agent-authored events are never held.
2. The hold adds no latency when the channel is busy.
3. `cancel_turn` with no in-flight turn drops that conversation's queued events
   instead of reporting `no_active_turn` and doing nothing.
4. A drained event's 👀 is removed.
5. The outcome is reported so the UI can be truthful.

## Files

| File | Change |
|---|---|
| `crates/buzz-acp/src/config.rs` | `dispatch_hold_ms`, default 2000, `0` disables. |
| `crates/buzz-acp/src/queue.rs` | Hold field + one predicate in the `flush_next` eligibility filter (`:311-320`). Same predicate in `has_flushable_work` (`:610`) so the two agree. |
| `crates/buzz-acp/src/lib.rs` | `handle_cancel_turn_control` (`:905-956`) falls through to a queue drain; clear 👀 for drained ids. |

## Steps

### 1. Hold as a single eligibility predicate

`flush_next` already filters to channels that are non-empty, not in-flight, and
not throttled (`queue.rs:311-320`). Add one more condition: the head event must
be older than the hold.

```rust
&& q.front().is_none_or(|qe| qe.received_at + self.dispatch_hold <= now)
```

`has_flushable_work` (`queue.rs:610`) applies the same filter and must get the
same condition, or the loop will spin reporting work it then refuses to flush.

**Do not add an "is the agent idle?" branch.** The in-flight check above already
supplies that behaviour: a busy channel's events are blocked anyway, so the hold
has elapsed by the time the channel frees. Adding an explicit idle test would be
a second source of truth for the same fact.

### 2. Human-only

Agent-to-agent traffic must not be held — agents do not mis-click. Set the
hold to zero for events whose author is a known agent, or gate at push time and
store the deadline per event. Prefer whichever reads closer to the existing
`author_allowed` / agent-identity helpers rather than introducing a new notion of
"human".

### 3. Cancel falls through to the queue

In `handle_cancel_turn_control` (`lib.rs:905-956`), today:

```rust
let fired = /* signal_in_flight_turn | signal_in_flight_task */;
let status = if fired { "sent" } else { "no_active_turn" };
```

When `fired` is false, drain the conversation's queued events before reporting:

```
if !fired:
    ids = queue.drain_channel(conversation_id.unwrap_or(channel_id))
    if !ids.is_empty():
        clear the 👀 for ids          # see step 4
        status = "cancelled_queued"
    else:
        status = "no_active_turn"     # genuinely nothing to stop
```

`drain_channel` (`queue.rs:649-667`) already removes the conversation's queue,
its `cancelled_batches`, its `withheld_native_steer`, and its retry throttle, and
**returns the dropped event ids for exactly this purpose** — its doc comment says
so. Do not hand-roll a second drain.

Keep `no_active_turn` for the truly-nothing case: criterion 4 needs the two
outcomes distinguishable.

### 4. Clear the 👀

Events get a 👀 at queue-push time (`lib.rs`, just after `queue.push`). A drained
event never runs, so the reaction must go — otherwise the timeline shows an agent
that read something it never received. `pool::clear_reactions` already exists for
the end-of-turn path; reuse it with the ids `drain_channel` returned.

### 5. Report the outcome

The existing `control_result` frame carries `status`. Add the `cancelled_queued`
value and the drained count. Phase 2 renders it; this phase must emit it whether
or not Phase 2 exists yet.

## Validation

Unit (`queue.rs`, alongside the existing flush tests):
- event younger than the hold → `flush_next` returns `None`; after the hold →
  returns it
- busy channel: event queued during a turn, turn completes after the hold has
  elapsed → dispatches immediately, no extra delay
- `has_flushable_work` and `flush_next` agree on a held event (no spin)
- `dispatch_hold_ms = 0` → current behaviour exactly
- agent-authored event → never held

Unit (`lib.rs`, alongside `message_edit_applied_tests`):
- cancel with queued-but-not-dispatched events → drained, `cancelled_queued`,
  no `turn_started` afterwards
- cancel with nothing anywhere → `no_active_turn`, nothing drained
- cancel during a running turn → unchanged from today (`ControlSignal::Cancel`)

Integration — `acp-harness-e2e` skill (real process, fake ACP agent):
- send, cancel within the hold, assert the fake agent is never prompted
- send, wait past the hold, assert it is prompted exactly once

Gate: `just ci`. Run `cd desktop && pnpm check` too if any desktop file moves —
`pnpm lint` alone does not catch formatting or the size ratchet.

## Risk / rollback

`dispatch_hold_ms = 0` restores today's behaviour exactly, so the hold is
revertible by config alone.

The sharpest risk is the two eligibility filters drifting: if `has_flushable_work`
says yes while `flush_next` says no, the loop spins every 500ms. The paired test
above is the guard, not a comment.
