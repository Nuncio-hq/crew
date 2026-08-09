import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";
import { classifyRelayClosed } from "@/shared/api/relayClosedPolicy";
import type { RelayEvent } from "@/shared/api/types";
import { fetchExhaustiveRelayHistory } from "@/shared/api/exhaustiveRelayPagination";
import {
  KIND_AGENT_RECEIPT,
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_RESOLVED,
  KIND_REACTION,
} from "@/shared/constants/kinds";

type FetchPage = (filter: RelaySubscriptionFilter) => Promise<RelayEvent[]>;

export function isPermanentHydrationError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  if (classifyRelayClosed(message) === "terminal") return true;
  const status = message.match(/\b(4\d{2})\b/)?.[1];
  if (status && !["408", "425", "429"].includes(status)) return true;
  return /\b(?:forbidden|unauthori[sz]ed|not authorized|restricted|blocked|invalid|proof[- ]of[- ]work|pow:|duplicate|unsupported|malformed|terminal error|access denied|policy rejected)\b/i.test(
    message,
  );
}

export function createHydrationRetryController<TTimer>({
  hydrate,
  onError,
  onPermanentError,
  retryDelayMs,
  setTimeoutFn,
  clearTimeoutFn,
  shouldRetry = (error) => !isPermanentHydrationError(error),
}: {
  hydrate: () => Promise<void>;
  onError: (error: unknown) => void;
  onPermanentError?: (error: unknown) => void;
  retryDelayMs: number;
  setTimeoutFn: (callback: () => void, delayMs: number) => TTimer;
  clearTimeoutFn: (timer: TTimer) => void;
  shouldRetry?: (error: unknown) => boolean;
}) {
  let stopped = false;
  let retryTimer: TTimer | undefined;
  let running: Promise<void> | null = null;
  let rerunRequested = false;

  const run = (): Promise<void> => {
    if (stopped) return Promise.resolve();
    if (running) {
      rerunRequested = true;
      return running;
    }
    running = hydrate()
      .then(() => {
        if (retryTimer !== undefined) {
          clearTimeoutFn(retryTimer);
          retryTimer = undefined;
        }
      })
      .catch((error) => {
        if (stopped) return;
        onError(error);
        if (!shouldRetry(error)) {
          rerunRequested = false;
          onPermanentError?.(error);
          return;
        }
        if (retryTimer !== undefined) return;
        retryTimer = setTimeoutFn(() => {
          retryTimer = undefined;
          void run();
        }, retryDelayMs);
      })
      .finally(() => {
        running = null;
        if (rerunRequested && !stopped) {
          rerunRequested = false;
          void run();
        }
      });
    return running;
  };

  return {
    run,
    stop() {
      stopped = true;
      rerunRequested = false;
      if (retryTimer !== undefined) {
        clearTimeoutFn(retryTimer);
        retryTimer = undefined;
      }
    },
  };
}

function byDurableOrder(left: RelayEvent, right: RelayEvent): number {
  return left.created_at - right.created_at || left.id.localeCompare(right.id);
}

function byUserInputAuthorityOrder(
  left: RelayEvent,
  right: RelayEvent,
): number {
  const leftPriority = left.kind === KIND_AGENT_USER_INPUT_REQUESTED ? 0 : 1;
  const rightPriority = right.kind === KIND_AGENT_USER_INPUT_REQUESTED ? 0 : 1;
  return leftPriority - rightPriority || byDurableOrder(left, right);
}

/** Merge a completed history snapshot with events buffered by its live overlap. */
export function mergeDurableActionEvents(
  userInputEvents: readonly RelayEvent[],
  receiptEvents: readonly RelayEvent[],
  reviewEvents: readonly RelayEvent[],
  bufferedEvents: readonly RelayEvent[],
) {
  const byId = new Map<string, RelayEvent>();
  for (const event of [
    ...userInputEvents,
    ...receiptEvents,
    ...reviewEvents,
    ...bufferedEvents,
  ]) {
    byId.set(event.id, event);
  }
  const merged = [...byId.values()];
  return {
    userInputEvents: merged
      .filter((event) =>
        [
          KIND_AGENT_USER_INPUT_REQUESTED,
          KIND_AGENT_USER_INPUT_ANSWER,
          KIND_AGENT_USER_INPUT_RESOLVED,
        ].includes(event.kind),
      )
      .sort(byUserInputAuthorityOrder),
    receiptEvents: merged
      .filter((event) => event.kind === KIND_AGENT_RECEIPT)
      .sort(byDurableOrder),
    reviewEvents: merged
      .filter((event) => event.kind === KIND_REACTION)
      .sort(byDurableOrder),
  };
}

/** Exhaustively enumerate immutable durable events without skipping timestamp ties. */
export async function enumerateDurableActionEvents(
  fetchPage: FetchPage,
  baseFilter: Omit<RelaySubscriptionFilter, "limit" | "since" | "until">,
  pageSize: number,
): Promise<RelayEvent[]> {
  if (!Number.isSafeInteger(pageSize) || pageSize <= 0) {
    throw new Error("Durable action page size must be a positive integer.");
  }
  return fetchExhaustiveRelayHistory(fetchPage, baseFilter, pageSize);
}
