# Plan — #110 Agent question card never renders in the channel

Spec: [#110](https://github.com/Nuncio-hq/crew/issues/110)
Status: **root cause proven locally**, fix not yet written.
Owner: CTO session (in flight)

## Outcome

An agent elicitation (`KIND_AGENT_USER_INPUT_REQUESTED`, 46040) published into a
channel renders its answer card in that channel, and `channels.spec.ts:500`
passes without any change to the test's assertions.

## Root cause (proven, not inferred)

Bisected locally in an isolated sibling worktree, single test, build verified
green each round:

| tree | result |
|---|---|
| `b57d26def` (#95) | **1 passed** |
| `25263120e` (#96) | **1 failed** — 4/4 runs, deterministic |
| `25263120e` with `desktop/src/app/AppShell.tsx` reverted to `b57d26def` | **1 passed** — 3/3 runs |

The failing change is 14 lines in `AppShell.tsx`, which now passes every channel
id into `useLiveHomeFeedActions`:

```ts
const liveHomeChannelIds = React.useMemo(
  () => (channelsQuery.data ?? []).map((c) => c.id),
  [channelsQuery.data],
);
useLiveHomeFeedActions(identityQuery.data?.pubkey, refetchHomeFeedFromLiveSignal, liveHomeChannelIds);
```

`useLiveHomeFeedActions` then opens an **app-wide** live subscription per channel
using `buildChannelUserInputFilter(channelId, 50, since)` — the same filter
`useChannelUserInput` uses for the channel card.

The failure mechanism is a **readiness-signal collision in the mock bridge**:

- `e2eBridge.ts:10436` `__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__` returns
  `hasMockLiveSubscription(channel.id, kind)` — a plain boolean, true if *any*
  subscription matches `(channelId, kind)`.
- The test clicks into `general`, then waits on that predicate before emitting.
- Since #96 the predicate is already satisfied by the **AppShell** subscription,
  which exists on every route as soon as channels load.
- So the test emits before `ChannelPane` → `useChannelUserInput` has subscribed.
  The mock delivers live events only to subscribers present at emit time, the
  channel hook misses it, and `hasCards` stays false. Card never renders.

Confirmed by instrumentation: the emitted event carries only `[["h", channelId]]`
(no `e` tag), and no page errors or exceptions occur. The
`if (!conversationId) return;` early-return in `useChannelUserInput.onEvent`
noted in the issue is **not** the cause — the id falls back to `event.id`, which
is valid, so the guard never trips.

## Blast radius — what this does and does not prove

**Test-visible today.** In production `useChannelUserInput.load()` subscribes and
*then* awaits `fetchEvents(filter)` for history, in that order, so a real relay
would return the request in history and the card would still appear. The mock
bridge does not replay runtime-emitted events into `fetchEvents`, which is why
only e2e sees it.

**Not proven safe.** Two real concerns remain and must not be waved away:

1. Every channel now carries a duplicate app-wide live subscription for the same
   filter. That is a deliberate Mission Inbox design choice in #96, not a defect
   — but it is unmeasured at realistic channel counts.
2. The mock bridge's live-only delivery hides a genuine ordering assumption. Any
   future code path that relies on the live event alone (no history refetch)
   would break in production the same way it breaks here.

## Options for the fix

Ranked. **Do not "fix" this by relaxing the assertions in `channels.spec.ts:500`**
— the user-facing property (card renders, is answerable) is correct and must stay
asserted verbatim.

1. **Make the mock bridge store emitted live events so `fetchEvents` replays them.**
   Raises fidelity to a real relay (a late subscriber's history fetch returns the
   event) and kills this whole race class for every spec, not just this one.
   Largest blast radius on shared test infra — must be run against the full smoke
   suite, not just this spec.
2. **Give the readiness helper a subscriber-count option** and have the test wait
   for the channel-level subscription specifically. Smallest change, but couples
   the test to a subscriber count that changes whenever the product adds a
   listener — it will rot.
3. **Emit after an explicit ChannelPane readiness marker.** Needs a new marker;
   mount is not the same as subscription-established, so this risks re-introducing
   the same race in a quieter form.

Recommendation: **(1)**, with the full smoke suite as the gate. Fall back to (2)
only if (1) destabilises other specs.

## Acceptance criteria

- `channels.spec.ts:500` passes, assertions unchanged.
- **Mutation check (required):** revert the fix line(s), re-run, and the test must
  go red again. A pass without this proves nothing.
- Full `pnpm test:e2e:smoke` shard 1 completes with no new failures.
- If option (1) is taken, state in the PR which other specs changed behaviour and why.

## Verification commands

```bash
cd desktop
pnpm build:e2e
pnpm exec playwright test --project=smoke tests/e2e/channels.spec.ts \
  --grep "channel question card accepts an answer" --retries=0
```

Build in a sibling worktree, never in the shared harness worktree — a concurrent
`pnpm build` silently corrupts a live Playwright run.

## Open questions

- Does option (1) change the meaning of any spec that currently *relies* on
  emitted events being invisible to a later `fetchEvents`? Must be checked before
  committing to it.
