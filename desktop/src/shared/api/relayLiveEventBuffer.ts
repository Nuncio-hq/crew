import type { RelayEvent } from "./types";
import type {
  RelayLiveEventContext,
  RelaySubscription,
} from "./relayClientShared";

export type BufferedLiveEvent = {
  subId: string;
  subscription: RelaySubscription;
  event: RelayEvent;
  context: RelayLiveEventContext;
};

export function toBufferedLiveEvent(
  subId: string,
  event: RelayEvent,
  subscription: RelaySubscription,
): BufferedLiveEvent {
  return {
    subId,
    subscription,
    event,
    context: {
      replay: subscription.mode === "live" && !subscription.ready,
    },
  };
}

export function dispatchBufferedLiveEvents(
  buffer: readonly BufferedLiveEvent[],
  subscriptions: ReadonlyMap<string, RelaySubscription>,
) {
  // Re-lookup: subscriptions removed during the batch window are skipped.
  for (const { subscription, event, context } of buffer) {
    const stillActive = [...subscriptions.values()].some(
      (candidate) => candidate === subscription,
    );
    if (stillActive && subscription.mode === "live") {
      subscription.onEvent(event, context);
    }
  }
}
