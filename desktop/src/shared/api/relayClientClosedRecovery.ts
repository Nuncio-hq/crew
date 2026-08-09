import { handleRelayClosed } from "@/shared/api/relayClosedRecovery";
import { recoverLiveSubscriptionHistory } from "@/shared/api/relayReconnectReplay";
import type {
  RelaySubscription,
  RelaySubscriptionFilter,
} from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";

export function handleSessionRelayClosed({
  subscriptions,
  subId,
  message,
  generation,
  isGenerationActive,
  sendReq,
  requestHistory,
}: {
  subscriptions: Map<string, RelaySubscription>;
  subId: string;
  message: string;
  generation: number;
  isGenerationActive: (generation: number) => boolean;
  sendReq: (subId: string, filter: RelaySubscriptionFilter) => Promise<void>;
  requestHistory: (filter: RelaySubscriptionFilter) => Promise<RelayEvent[]>;
}) {
  handleRelayClosed({
    subscriptions,
    subId,
    message,
    sendReq,
    isActive: () => isGenerationActive(generation),
    recoverHistory: async (activeSubId, subscription) => {
      return recoverLiveSubscriptionHistory({
        subscription,
        now: Math.floor(Date.now() / 1_000),
        isActive: () =>
          isGenerationActive(generation) &&
          subscriptions.get(activeSubId) === subscription,
        requestHistory,
      });
    },
  });
}
