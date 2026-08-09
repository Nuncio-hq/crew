import type {
  RelayLiveEventContext,
  RelayLiveSubscriptionStatus,
  RelaySubscription,
  RelaySubscriptionFilter,
} from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";
import {
  clearClosedRetry,
  deleteSubscriptionAliases,
} from "./relayClosedRecovery";
import { LIVE_SUBSCRIPTION_READY_TIMEOUT_MS } from "./relayClientTimings";

export async function establishLiveSubscription({
  subscriptions,
  subId,
  filter,
  onEvent,
  onStatus,
  recoveryFloorCreatedAt,
  sendRequest,
  closeSubscription,
}: {
  subscriptions: Map<string, RelaySubscription>;
  subId: string;
  filter: RelaySubscriptionFilter;
  onEvent: (event: RelayEvent, context: RelayLiveEventContext) => void;
  onStatus?: (status: RelayLiveSubscriptionStatus) => void;
  recoveryFloorCreatedAt: number;
  sendRequest: () => Promise<void>;
  closeSubscription: (subId: string) => Promise<void>;
}): Promise<void> {
  let resolveReady = () => {};
  let rejectReady = (_error: Error) => {};
  const ready = new Promise<void>((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });
  subscriptions.set(subId, {
    mode: "live",
    currentSubId: subId,
    filter,
    onEvent,
    onStatus,
    ready: false,
    resolveReady,
    rejectReady,
    recoveryFloorCreatedAt,
  });
  let rejectDeadline = (_error: Error) => {};
  const deadline = new Promise<never>((_resolve, reject) => {
    rejectDeadline = reject;
  });
  const setupTimeout = window.setTimeout(() => {
    rejectDeadline(
      new Error(
        `Relay subscription readiness timed out after ${LIVE_SUBSCRIPTION_READY_TIMEOUT_MS}ms`,
      ),
    );
  }, LIVE_SUBSCRIPTION_READY_TIMEOUT_MS);
  let request: Promise<void> | undefined;
  try {
    request = sendRequest();
    await Promise.race([Promise.all([request, ready]), deadline]);
  } catch (error) {
    const active = subscriptions.get(subId);
    if (active?.mode === "live") {
      deleteSubscriptionAliases(subscriptions, active);
      clearClosedRetry(active);
      active.onStatus?.({
        state: "closed",
        message: error instanceof Error ? error.message : String(error),
      });
      const wireSubId = active.currentSubId ?? subId;
      const closeAfterFailure = () => closeSubscription(wireSubId);
      void closeAfterFailure().catch((closeError) => {
        console.warn(
          "Failed to close timed-out relay subscription",
          closeError,
        );
      });
      void request?.then(closeAfterFailure).catch(() => {});
    }
    throw error;
  } finally {
    window.clearTimeout(setupTimeout);
  }
}
