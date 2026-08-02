# Phase 2 — Desktop: Stop where the working indicator already is

**App:** `desktop/`
**Requires:** Phase 1. Without it, Stop pressed during the hold is a silent no-op.

## Context

The Stop capability already exists end to end — it is just buried. `cancelManagedAgentTurn`
(`desktop/src/shared/api/agentControl.ts:10`) sends `cancel_turn`, and the only UI
that calls it is an item inside a dropdown behind a Settings gear in the Agent
Session panel (`desktop/src/features/channels/ui/AgentSessionThreadPanel.tsx:265`,
rendered around `:414` / `:435`).

The job is to move it to where people already look: the composer activity rail
that shows which agents are working
(`desktop/src/features/channels/ui/ChannelComposerActivityAccessory.tsx`, which
renders `BotActivityComposerAction`).

## Requirements

1. Stop is reachable in one click from the channel/thread view whenever an agent
   has work pending or running for that conversation.
2. It targets the conversation the person is looking at, not "some agent".
3. Every press produces visible feedback, including the nothing-to-stop case.
4. No new state machine — reuse what the observer already provides.

## Files

| File | Change |
|---|---|
| `desktop/src/features/channels/ui/ChannelComposerActivityAccessory.tsx` | Stop affordance beside the working indicator. |
| `desktop/src/features/channels/ui/BotActivityBar.tsx` (`BotActivityComposerAction`) | Per-agent stop when more than one agent is active in the conversation. |
| `desktop/src/features/agents/` | Small selector for "has queued-or-running work in this conversation". |
| `desktop/src/features/channels/ui/AgentSessionThreadPanel.tsx` | Keep the gear item only for the All-channels scope the composer cannot reach. |

## Steps

1. **Targeting.** `useThreadComposerBotActivity` already yields the working agent
   pubkeys for the open thread, and `getActiveTurnControlTargetsForAgent`
   (`desktop/src/features/agents/activeAgentTurnsStore.ts:480`) yields
   `{channelId, conversationId, turnId}`. Filter those targets by the current
   conversation. No backend work for the running case.

2. **The queued case.** During the hold there is no `turnId`. The `cancel_turn`
   frame already treats `turnId` as optional and falls back to the conversation —
   send it without one. Phase 1 makes that drain the queue.

   Reuse v2's derivation for "queued": an event id absent from every
   `turn_started.triggeringEventIds` is not yet dispatched
   (`desktop/src/features/agents/dispatchedEventIds.ts`). That module already
   exists and is already reset on community switch; extend it rather than adding
   a parallel store.

3. **One agent vs several.** With a single active agent, Stop acts on it. With
   several in the same conversation, the rail already renders them individually —
   hang the control on each rather than inventing a "stop all", which is the kind
   of button people press once and regret.

4. **Feedback for every press.** Render the `control_result` status:
   `sent` → stopping, `cancelled_queued` → request withdrawn before the agent saw
   it, `no_active_turn` → nothing to stop. The third case is today's silent
   `tracing::warn` and is the one most worth fixing.

5. **Community reset.** Any new module-level cache must be registered in
   `resetCommunityState()` (`desktop/src/features/communities/useCommunityInit.ts`)
   per CLAUDE.md. If the selector lives inside `dispatchedEventIds`, its reset is
   already wired through `resetAgentObserverStore`.

6. **Reference stability.** Wrap derived sets in the content-equality cache
   (`desktop/src/shared/hooks/useStableReference.ts`) — a fresh `Set` per render
   defeats `React.memo` down the timeline (CLAUDE.md gotcha 7).

## Validation

Unit (`.test.mjs`, colocated):
- selector reports work for a conversation with a queued event and no turn
- selector reports work for a running turn
- selector is reference-stable across unrelated observer updates
- each `control_result` status maps to a distinct rendered state

E2E (`desktop/tests/e2e/`, registered in `playwright.config.ts`):
- build with `pnpm build:e2e` / `pnpm test:e2e:smoke`, never `pnpm run build` —
  a plain build strips the mock bridge and every mock-mode spec fails in a way
  that looks like a product bug
- Stop visible while an agent works, gone when it stops

Screenshots for the PR: `just desktop-screenshot`, posted with
`scripts/post-screenshots.sh` (never `buzz upload` — relay URLs die in GitHub's
camo proxy). Check `shasum -a 256` on the PNGs before posting; identical hashes
mean two shots captured the same state.

Gate: `just ci`, plus `cd desktop && pnpm check`.

## Risk / rollback

UI-only; reverting removes the control and leaves Phase 1's behaviour intact.

Main risk is the control appearing when there is nothing to stop, which trains
people to distrust it. The selector must be driven by real observer state, not by
optimism.
