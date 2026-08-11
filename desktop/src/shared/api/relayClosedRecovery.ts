import { classifyRelayClosed } from "@/shared/api/relayClosedPolicy";
import {
  activateRateLimit,
  parseRateLimitHint,
  rateLimitRemainingMs,
} from "@/shared/api/relayRateLimitGate";
import {
  sortEvents,
  type RelaySubscription,
  type RelaySubscriptionFilter,
} from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";

export function isCurrentLiveWireId(
  subscription: RelaySubscription,
  subId: string,
): boolean {
  return (
    subscription.mode !== "live" ||
    subscription.currentSubId === undefined ||
    subscription.currentSubId === subId
  );
}

export function deleteSubscriptionAliases(
  subscriptions: Map<string, RelaySubscription>,
  target: RelaySubscription,
): void {
  for (const [id, subscription] of subscriptions) {
    if (subscription === target) subscriptions.delete(id);
  }
}

const RETRY_BASE_DELAY_MS = 1_000;
const RETRY_MAX_DELAY_MS = 30_000;

type LiveSubscription = Extract<RelaySubscription, { mode: "live" }>;

function markLiveSubscriptionOpen(subscription: LiveSubscription) {
  subscription.ready = true;
  subscription.onStatus?.({ state: "open" });
  subscription.resolveReady?.();
  subscription.resolveReady = undefined;
  subscription.rejectReady = undefined;
  subscription.closedRetryAttempt = 0;
  clearClosedRetry(subscription);
}

/** Begin one generation-scoped live/history recovery barrier. */
export function beginLiveSubscriptionRecovery(
  subscription: LiveSubscription,
): number {
  const generation = (subscription.recoveryGeneration ?? 0) + 1;
  subscription.recoveryGeneration = generation;
  subscription.ready = false;
  subscription.recoveryInFlight = true;
  subscription.recoveryRequestSent = false;
  subscription.recoveryEoseReceived = false;
  return generation;
}

/** Attribute EOSE only after this generation's replacement REQ was dispatched. */
export function markLiveRecoveryRequestSent(
  subscription: LiveSubscription,
  generation: number,
) {
  if (subscription.recoveryGeneration !== generation) return false;
  subscription.recoveryRequestSent = true;
  return true;
}

/** Complete history only for the current generation; stale cycles stay closed. */
export function completeLiveSubscriptionRecovery(
  subscription: LiveSubscription,
  generation: number,
  completed: boolean,
) {
  if (subscription.recoveryGeneration !== generation || !completed)
    return false;
  subscription.recoveryInFlight = false;
  if (subscription.recoveryEoseReceived) {
    markLiveSubscriptionOpen(subscription);
  }
  return true;
}

export function clearClosedRetry(subscription: LiveSubscription) {
  if (subscription.closedRetryTimeout === undefined) return;
  window.clearTimeout(subscription.closedRetryTimeout);
  subscription.closedRetryTimeout = undefined;
}

export function handleRelayClosed({
  subscriptions,
  subId,
  message,
  sendReq,
  recoverHistory,
  isActive,
}: {
  subscriptions: Map<string, RelaySubscription>;
  subId: string;
  message: string;
  sendReq: (subId: string, filter: RelaySubscriptionFilter) => Promise<void>;
  recoverHistory?: (
    subId: string,
    subscription: LiveSubscription,
  ) => Promise<boolean>;
  isActive?: () => boolean;
}) {
  const subscription = subscriptions.get(subId);
  if (!subscription) return;
  if (subscription.mode !== "live") {
    // Classify before rejecting so a `rate-limited:` history CLOSED arms the
    // gate for concurrent ops. A history sub can't be retried (the caller holds
    // the promise), so we still reject immediately after arming.
    const closedClass = classifyRelayClosed(message);
    if (closedClass === "rate-limited") {
      const hintSeconds = parseRateLimitHint(message);
      activateRateLimit(hintSeconds);
    }
    window.clearTimeout(subscription.timeout);
    subscriptions.delete(subId);
    subscription.reject(
      new Error(message || "Relay closed the history subscription."),
    );
    return;
  }
  if (!isCurrentLiveWireId(subscription, subId)) return;
  recoverLiveSubscriptionFromClosed({
    subscriptions,
    subId,
    subscription,
    message,
    sendReq,
    recoverHistory,
    isActive,
  });
}

function recoverLiveSubscriptionFromClosed({
  subscriptions,
  subId,
  subscription,
  message,
  sendReq,
  recoverHistory,
  isActive,
}: {
  subscriptions: Map<string, RelaySubscription>;
  subId: string;
  subscription: LiveSubscription;
  message: string;
  sendReq: (subId: string, filter: RelaySubscriptionFilter) => Promise<void>;
  recoverHistory?: (
    subId: string,
    subscription: LiveSubscription,
  ) => Promise<boolean>;
  isActive?: () => boolean;
}) {
  subscription.ready = false;

  const closedClass = classifyRelayClosed(message);
  subscription.onStatus?.({
    state: closedClass === "terminal" ? "closed" : "recovering",
    message: message || "Relay closed the live subscription.",
  });

  if (closedClass === "terminal") {
    // Auth/access/filter failure — permanently remove the subscription so it
    // doesn't silently loop.
    subscription.rejectReady?.(
      new Error(message || "Relay rejected the live subscription."),
    );
    subscription.resolveReady = undefined;
    subscription.rejectReady = undefined;
    deleteSubscriptionAliases(subscriptions, subscription);
    return;
  }
  const recoveryGeneration = beginLiveSubscriptionRecovery(subscription);
  const baseSubId = subscription.baseSubId ?? subId;
  const replacementSubId = `${baseSubId}:recovery:${recoveryGeneration}`;
  subscription.baseSubId = baseSubId;
  for (const [id, candidate] of subscriptions) {
    if (candidate === subscription && id !== baseSubId)
      subscriptions.delete(id);
  }
  subscription.currentSubId = replacementSubId;
  subscriptions.set(replacementSubId, subscription);
  clearClosedRetry(subscription);

  const attempt = subscription.closedRetryAttempt ?? 0;
  const backoffMs = Math.min(
    RETRY_BASE_DELAY_MS * 2 ** attempt,
    RETRY_MAX_DELAY_MS,
  );

  let delayMs = backoffMs;

  if (closedClass === "rate-limited") {
    // Activate the gate so concurrent operations back off too.
    const hintSeconds = parseRateLimitHint(message);
    activateRateLimit(hintSeconds);
    // Use the gate's actual remaining time so a shorter hint arriving under a
    // longer active gate does not schedule a premature retry that just gets
    // another CLOSED. The fallback covers the gate-inactive edge case
    // (hint * 1000, or 10s default when no hint).
    const fallbackMs = (hintSeconds ?? 10) * 1_000;
    delayMs = Math.max(backoffMs, rateLimitRemainingMs() || fallbackMs);
  }

  subscription.closedRetryAttempt = attempt + 1;
  subscription.closedRetryTimeout = window.setTimeout(() => {
    subscription.closedRetryTimeout = undefined;
    if (
      subscriptions.get(replacementSubId) !== subscription ||
      subscription.recoveryGeneration !== recoveryGeneration ||
      isActive?.() === false
    ) {
      return;
    }
    void sendReq(replacementSubId, subscription.filter)
      .then(() => {
        if (!markLiveRecoveryRequestSent(subscription, recoveryGeneration)) {
          return false;
        }
        if (isActive?.() === false) return false;
        return recoverHistory
          ? recoverHistory(replacementSubId, subscription)
          : true;
      })
      .then((completed) => {
        if (subscriptions.get(replacementSubId) !== subscription) return;
        completeLiveSubscriptionRecovery(
          subscription,
          recoveryGeneration,
          completed !== false,
        );
      })
      .catch((error) => {
        if (
          subscriptions.get(replacementSubId) !== subscription ||
          subscription.recoveryGeneration !== recoveryGeneration ||
          isActive?.() === false
        ) {
          return;
        }
        console.error("Failed to restore closed relay subscription", error);
        const recoveryMessage =
          error instanceof Error ? error.message : String(error);
        recoverLiveSubscriptionFromClosed({
          subscriptions,
          subId: replacementSubId,
          subscription,
          message: recoveryMessage,
          sendReq,
          recoverHistory,
          isActive,
        });
      });
  }, delayMs);
}

export function prepareSubscriptionEvent(
  subscription: RelaySubscription,
  event: RelayEvent,
) {
  if (subscription.mode === "history") {
    subscription.events.push(event);
    return false;
  }
  if (subscription.mode === "first") {
    return false;
  }
  subscription.closedRetryAttempt = 0;
  clearClosedRetry(subscription);
  subscription.lastSeenCreatedAt = Math.max(
    subscription.lastSeenCreatedAt ?? 0,
    event.created_at,
  );
  return true;
}

export function handleSubscriptionEose({
  subscriptions,
  subId,
  closeSubscription,
}: {
  subscriptions: Map<string, RelaySubscription>;
  subId: string;
  closeSubscription: (subId: string) => Promise<void>;
}) {
  const subscription = subscriptions.get(subId);
  if (!subscription) return;
  if (subscription.mode === "live") {
    if (!isCurrentLiveWireId(subscription, subId)) return;
    if (subscription.recoveryInFlight) {
      // The replacement wire id is generation-unique, so an early EOSE is
      // already authoritative even if the send promise has not resolved yet.
      subscription.recoveryEoseReceived = true;
      return;
    }
    markLiveSubscriptionOpen(subscription);
    return;
  }
  window.clearTimeout(subscription.timeout);
  subscriptions.delete(subId);
  void closeSubscription(subId);
  if (subscription.mode === "first") {
    subscription.resolve(null);
  } else {
    subscription.resolve(sortEvents(subscription.events));
  }
}
