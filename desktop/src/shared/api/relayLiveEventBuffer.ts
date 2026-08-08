import type { RelayEvent } from "./types";
import type {
  RelayLiveEventContext,
  RelaySubscription,
} from "./relayClientShared";

export type BufferedLiveEvent = {
  subId: string;
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
  for (const { subId, event, context } of buffer) {
    const subscription = subscriptions.get(subId);
    if (subscription?.mode === "live") subscription.onEvent(event, context);
  }
}
