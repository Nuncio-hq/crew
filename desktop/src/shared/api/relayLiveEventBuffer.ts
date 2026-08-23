import type { RelayEvent } from "./types";
import type {
  RelayLiveEventContext,
  RelaySubscription,
} from "./relayClientShared";
import { shouldDispatchSubscriptionEvent } from "./relayClosedRecovery";

export type BufferedLiveEvent = {
  subId: string;
  subscription: RelaySubscription;
  event: RelayEvent;
  context: RelayLiveEventContext;
  /** Connection generation the event arrived on; stale frames are dropped. */
  generation: number;
};

export function toBufferedLiveEvent(
  subId: string,
  event: RelayEvent,
  subscription: RelaySubscription,
  generation: number,
): BufferedLiveEvent {
  return {
    subId,
    subscription,
    event,
    context: {
      replay: subscription.mode === "live" && !subscription.ready,
    },
    generation,
  };
}

export function dispatchBufferedLiveEvents(
  buffer: readonly BufferedLiveEvent[],
  subscriptions: ReadonlyMap<string, RelaySubscription>,
  generation: number,
) {
  // Re-lookup: subscriptions removed during the batch window are skipped, and
  // frames buffered on a superseded connection generation are dropped — the
  // new connection's reconnect repair re-delivers anything they carried.
  for (const item of buffer) {
    const { subscription, event, context } = item;
    if (item.generation !== generation) continue;
    const stillActive = [...subscriptions.values()].some(
      (candidate) => candidate === subscription,
    );
    if (!stillActive || subscription.mode !== "live") continue;
    if (!shouldDispatchSubscriptionEvent(subscription, event)) continue;
    subscription.onEvent(event, context);
  }
}
