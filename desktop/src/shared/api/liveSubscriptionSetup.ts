import type {
  RelayLiveEventContext,
  RelayLiveSubscriptionStatus,
  RelaySubscription,
  RelaySubscriptionFilter,
} from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";
import { classifyRelayClosed } from "./relayClosedPolicy";
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
  readinessTimeoutMs = LIVE_SUBSCRIPTION_READY_TIMEOUT_MS,
  recoveryFloorCreatedAt,
  sendRequest,
  closeSubscription,
}: {
  subscriptions: Map<string, RelaySubscription>;
  subId: string;
  filter: RelaySubscriptionFilter;
  onEvent: (event: RelayEvent, context: RelayLiveEventContext) => void;
  onStatus?: (status: RelayLiveSubscriptionStatus) => void;
  readinessTimeoutMs?: number;
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
  let readinessTimedOut = false;
  const deadline = new Promise<never>((_resolve, reject) => {
    rejectDeadline = reject;
  });
  const setupTimeout = window.setTimeout(() => {
    readinessTimedOut = true;
    rejectDeadline(
      new Error(
        `Relay subscription readiness timed out after ${readinessTimeoutMs}ms`,
      ),
    );
  }, readinessTimeoutMs);
  let request: Promise<void> | undefined;
  let requestSettled = false;
  try {
    request = sendRequest().finally(() => {
      requestSettled = true;
    });
    await Promise.race([Promise.all([request, ready]), deadline]);
  } catch (error) {
    if (readinessTimedOut) {
      console.warn(
        `Relay subscription readiness remains pending: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
      onStatus?.({
        state: "recovering",
        message: error instanceof Error ? error.message : String(error),
      });
      // The REQ may still be in flight and the relay may still send EOSE. Keep
      // the exact live owner installed so eventual readiness and buffered live
      // events remain authoritative. A late send failure still terminalizes
      // this owner through the normal failure path below.
      void request?.catch((lateError) => {
        const active = subscriptions.get(subId);
        if (
          active?.mode === "live" &&
          (active.currentSubId ?? subId) === subId
        ) {
          const message =
            lateError instanceof Error ? lateError.message : String(lateError);
          const terminal = classifyRelayClosed(message) === "terminal";
          if (terminal) {
            deleteSubscriptionAliases(subscriptions, active);
            clearClosedRetry(active);
          } else {
            active.ready = false;
          }
          active.onStatus?.({
            state: terminal ? "closed" : "recovering",
            message,
          });
        }
      });
      return;
    }
    const active = subscriptions.get(subId);
    if (active?.mode === "live") {
      if ((active.currentSubId ?? subId) === subId) {
        deleteSubscriptionAliases(subscriptions, active);
        clearClosedRetry(active);
        active.onStatus?.({
          state:
            classifyRelayClosed(
              error instanceof Error ? error.message : String(error),
            ) === "terminal"
              ? "closed"
              : "recovering",
          message: error instanceof Error ? error.message : String(error),
        });
      } else if (subscriptions.get(subId) === active) {
        subscriptions.delete(subId);
      }
    }
    const closeOriginalWire = () =>
      closeSubscription(subId).catch((closeError) => {
        console.warn(
          "Failed to close timed-out relay subscription",
          closeError,
        );
      });
    if (requestSettled) void closeOriginalWire();
    else {
      // The REQ may be written after the timeout. Close its exact wire once,
      // after that send settles, without touching any recovery replacement.
      void request?.then(closeOriginalWire).catch(() => {});
    }
    throw error;
  } finally {
    window.clearTimeout(setupTimeout);
  }
}
