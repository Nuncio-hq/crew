# Phase 2 — Desktop: the editable window on the message

**App:** `desktop/`
**Requires:** Phase 1 merged. Shipping this first produces a label with no
behaviour behind it.

## Context

Desktop already receives everything needed to know whether a message has reached
the agent. `turn_started` carries `triggeringEventIds` (`crates/buzz-acp/src/pool.rs:1402-1416`)
and the transcript store already ingests and indexes them per turn
(`desktop/src/features/agents/ui/agentSessionTranscript.ts:149-158`,
`agentSessionTranscriptHelpers.ts:435-437`).

So the dispatch boundary is derivable today. What is missing is exposing it
outside the agent-session panel, where the message timeline can read it.

## Requirements

1. A message addressed to an agent, still queued, is visibly editable-as-undo.
2. The moment its id appears in a `turn_started`, that framing disappears.
3. If Phase 1 reports `applied: false`, the UI corrects itself — the user is told
   the agent already read the original.
4. No new state machine. Derive from what the observer already provides.

## Files

| File | Change |
|---|---|
| `desktop/src/features/agents/` (new small module, e.g. `dispatchedEventIds.ts`) | Selector: has event id `X` appeared in any `turn_started.triggeringEventIds`? Reads the existing transcript store; do not duplicate ingestion. |
| `desktop/src/features/messages/ui/ComposerReplyEditBanner.tsx` | When editing a still-queued message, label the action as undo rather than plain edit. |
| Message actions / edit entry point in `desktop/src/features/messages/ui/` | Same derived state gates the affordance. |
| `desktop/src/features/messages/hooks.ts` (`:690-745`) | On `applied: false`, surface the too-late state on the message. No change to the edit mutation itself. |

## Steps

1. **Selector, not new plumbing.** One function over the existing transcript
   store: `isEventDispatched(eventId)`. The store is already keyed per turn;
   this flattens the ids it holds. Wrap in the content-equality ref cache
   (`shared/hooks/useStableReference.ts`) — a fresh `Set` per render defeats
   `React.memo` down the timeline (see CLAUDE.md gotcha 7).

2. **Three states, derived, not stored.**

   | State | Condition | UI |
   |---|---|---|
   | Queued | message mentions an agent, id not in any `turn_started` | edit reads as undo |
   | Dispatched | id present in a `turn_started` | ordinary edit, no undo language |
   | Cancelled | `outcome: "dropped"` — the edit removed the mention | inline: request withdrawn, agent never ran |
   | Too late | `applied: false` | inline: agent already read the original |

   Per Oscar's call, the last two are **inline on the message**, not toasts —
   both are facts about whether work happened, and a missed toast leaves the
   user with a wrong belief.

3. **Optimistic, then corrected.** Desktop shows the queued affordance from its
   own derivation, and `message_edit_applied` is the authority. When they
   disagree the harness wins — that is the whole point of Phase 1 emitting it.

4. **Community reset.** If the selector introduces a module-level cache, register
   its reset in `resetCommunityState()`
   (`desktop/src/features/communities/useCommunityInit.ts`) per CLAUDE.md. A
   cache of dispatched ids from the previous relay leaking into a new community
   would mark unrelated messages as already-read.

5. **Wording.** The undo framing must not appear on messages that mention no
   agent — for a human-to-human message the concept is meaningless.

## Validation

Unit (`.test.mjs`, colocated — matches existing store test style):
- id absent from all turns → queued
- id present in one turn → dispatched
- `applied: false` overrides a locally-queued derivation
- selector returns a reference-stable value across unrelated store updates

E2E (`desktop/tests/e2e/`, registered in `playwright.config.ts`, per CLAUDE.md):
- build with `pnpm build:e2e` / `pnpm test:e2e:smoke`, never `pnpm run build`
- seed a message, assert the undo affordance, emit a `turn_started` carrying its
  id, assert the affordance is gone

Screenshots for the PR: `just desktop-screenshot`, posted via
`scripts/post-screenshots.sh` (never `buzz upload` — relay URLs die in GitHub's
camo proxy). Verify distinct hashes before posting:
`shasum -a 256 test-results/screenshots/*.png`.

Gate: `just ci`.

## Risk / rollback

UI-only; revert removes the affordance and leaves Phase 1 behaviour intact
(edits still apply to queued messages, just without the label).

Main risk is the derivation drifting from truth if `turn_started` is missed
(observer gap, app opened mid-turn). Failure mode is showing undo framing on an
already-dispatched message — Phase 1's `applied: false` catches it after the fact,
which is why the too-late state is required, not optional.
