# Crew Vision

> **Status: historical vision (2026-08-10).** This document pre-dates
> [`FOUNDER-PRODUCT.md`](FOUNDER-PRODUCT.md) and is kept as history; it is not
> rewritten. The section [Board as orchestrator](#board-as-orchestrator-superseded)
> is **superseded** as a product commitment by **D-037** in
> [`DECISIONS.md`](DECISIONS.md): channels and threads are the surface where
> work happens, and board-as-home is not current direction. Every other section
> — manager–agent relationship, mention-as-assignment, thin-fork constraints,
> board state as signed relay events — remains aligned and in force.

## Product statement

Crew is mission control for a team of agents.

It is not a chat IDE, code editor, copilot, or a place where the manager watches
an agent type. The intended experience is closer to managing teammates in a
room:

1. The manager raises an idea or issue.
2. An agent investigates independently.
3. The agent asks for a meeting when judgment is needed.
4. The discussion produces an agreed plan.
5. Work continues asynchronously.
6. The manager returns only for decisions, review, or completion.

Chat is the detail view of work. The board is the control surface.
(**Superseded by D-037**: channel-first stands; the channel/thread is the
surface, not a board.)

## Manager and agent relationship

The manager provides intent, constraints, and judgment. Agents are expected to:

- research before proposing implementation;
- expose disagreement and uncertainty;
- preserve resumable sessions instead of holding processes open;
- ask only when human judgment is actually required;
- produce evidence, plans, tests, and artifacts that can be reviewed later;
- work without requiring the manager to supervise execution.

An agent has its own keypair and is treated as a participant. Mentioning an
agent assigns work. A future mention syntax may also select model, reasoning,
speed, or other execution attributes.

## Board as orchestrator (superseded)

> **Superseded by D-037** ([`DECISIONS.md`](DECISIONS.md)). Board-as-home —
> columns as authoritative state, a `Working` cap, card-move-as-transition —
> is not current direction and no board event kind or tag schema is planned.
> Kept here for history only.

The primary workflow is:

```text
Issues -> Planned -> Working -> Need Input -> Done
```

The column is authoritative state. Moving a card is a workflow transition, not
a cosmetic operation.

Rules:

- `Working` has a hard cap of three cards.
- `Need Input` does not consume a `Working` slot.
- `Need Input` has higher manager priority than every other state.
- When a slot becomes available, eligible `Planned` work may enter `Working`.
- A card moves to `Need Input` only when human input is required to proceed.
- Resuming after input must preserve the task and session history.

## Work lifecycle

1. The manager pins an idea or issue into `Issues`.
2. The CTO agent researches, creates or resumes a session, and reports back.
3. The manager joins the asynchronous meeting when convenient.
4. Agreement produces a plan and moves the card to `Planned`.
5. Capacity control moves the card to `Working`.
6. Agent mentions delegate bounded work to specialist agents.
7. Human judgment moves the card to `Need Input` and releases capacity.
8. Verified completion moves the card to `Done`.

Clicking a card opens its conversation. Conversation is scoped to the card; it
does not replace the board.

## Locked product constraints

- Crew remains a fork of Buzz.
- Existing Buzz styling is not replaced.
- New desktop UI is TypeScript/React inside the existing Tauri app.
- Prefer adding files. Existing upstream-file edits must stay exceptionally
  small and easy to replay.
- Board state is represented by signed relay events, not React-only state or a
  separate application database.
- Project identity follows NIP-34: `(pubkey, identifier)`.
- Clone URLs and local paths are locations, never identity.
- Source code remains on the local filesystem.
- Event data travels through the local relay over WebSocket.
- Media artifacts use the media store and are referenced by URL.
- Agent execution uses subscription-backed tools, not metered API keys.

## What this vision does not decide

This document does not freeze:

- the final board event kind;
- the exact local-location tag schema;
- a future mobile implementation;
- per-Project ACP `cwd`;
- final model-selection syntax in mentions.

Those require focused spikes and explicit decisions.
