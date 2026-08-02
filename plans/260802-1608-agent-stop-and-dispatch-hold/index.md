# Stop button + idle-only dispatch hold — steering v1

**Status:** approved, not started (Oscar 2026-08-02: "làm (a) + (b)")
**Follows:** [edit-as-undo v2](../260802-1456-agent-edit-as-undo/index.md), merged as `1501192e2`.

## Outcome

A person can stop an agent from the place they are already looking, and a
mis-sent message has a window in which nothing has run yet.

v2 shipped the *edit* path. It does not cover either of these — see below.

## Why v2 is not enough (measured, not assumed)

The flush loop ticks every **500ms** (`crates/buzz-acp/src/lib.rs:478`) with no
debounce, and `flush_next` only skips channels that are in-flight or throttled
(`crates/buzz-acp/src/queue.rs:311-320`).

| Agent state | Real editable/undo window today |
|---|---|
| Idle | ~500ms — effectively zero |
| Busy with another turn | Length of that turn (minutes) |

So today's protection is strongest exactly when it is least needed, and absent
exactly when it is most needed: a mis-click almost always lands on an idle
agent. After dispatch there is nothing at all but `!cancel`, which is invisible.

## Scope

| # | Item | Phase |
|---|---|---|
| a | Stop control on the composer activity rail | [phase-02](phase-02-desktop-stop-control.md) |
| b | Dispatch hold, only costly when the agent is idle | [phase-01](phase-01-harness-hold-and-cancel.md) |
| c | Cancel must also cancel a *queued* request, not just an in-flight turn | [phase-01](phase-01-harness-hold-and-cancel.md) |

(c) is not extra scope — it is what makes (a) and (b) work together. Without it
the Stop button is a no-op during the entire hold window.

### The single-predicate insight

The hold is one added condition in the `flush_next` eligibility filter: skip
events younger than the hold. It does **not** need an "is the agent idle?"
branch.

When the channel is busy the event is already blocked by the existing in-flight
check, so by the time the turn frees up the hold has long since elapsed and adds
nothing. The hold therefore only ever costs latency in the idle case — which is
precisely the case where the window is otherwise ~500ms. One predicate buys both
behaviours.

### The trap: Stop during the hold is a no-op today

`handle_cancel_turn_control` signals an **in-flight** turn and reports
`no_active_turn` when there is none (`lib.rs:929-939`). During the hold there is
no turn yet, so a Stop button wired straight to the existing frame would do
nothing and say nothing useful — the exact silent-no-op failure this whole line
of work exists to remove.

Cancel must fall through to dropping the queued events for that conversation.

## Non-goals

Explicitly dropped after review, do not build:

- **Send→Stop morph on the send button.** More complexity than value once a Stop
  control sits next to the working indicator, and it fights the fact that in
  Buzz sending during a turn is how steering works.
- **Side-effect ledger**, **retry classification**, **Interrupt as a UI action.**
  No user signal demands them yet; revisit when people are actually stopping
  things.

## Acceptance criteria

1. Idle agent, message sent, Stop pressed within the hold → no turn ever starts.
   No `turn_started` for that event.
2. Busy agent → the hold adds **no** measurable latency; the event dispatches as
   soon as the prior turn completes.
3. Stop during a running turn keeps working exactly as today (`ControlSignal::Cancel`,
   batch dropped, session preserved).
4. Stop with nothing queued and nothing running tells the user so, rather than
   failing silently.
5. Agent-to-agent traffic is not held.
6. A cancelled-while-queued message loses its 👀 — no eyes left on work that
   never ran.

## Open question for Oscar

Hold duration. The plan uses **2s** as the default. It is the only user-visible
tunable here, and it only ever applies to an idle agent, so 2s is the whole added
latency in that case. Ship it as config so the number can move without a code
change.
