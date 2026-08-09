import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { remindersQueryKey } from "@/features/reminders/hooks";
import { relayClient } from "@/shared/api/relayClient";
import {
  KIND_APPROVAL_REQUEST,
  KIND_APPROVAL_GRANT,
  KIND_APPROVAL_DENY,
  KIND_EVENT_REMINDER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_RESOLVED,
  KIND_AGENT_RECEIPT,
  KIND_REACTION,
} from "@/shared/constants/kinds";
import { getThreadReference } from "@/features/messages/lib/threading";
import {
  beginExhaustiveApprovalProjection,
  endExhaustiveApprovalProjection,
  ingestApprovalRequestEvent,
  resolveApprovalRequestEvent,
} from "@/features/agents/needsYouStore";
import {
  beginExhaustiveUserInputProjection,
  endExhaustiveUserInputProjection,
  isUserInputAttentionProjectionUnavailable,
  markUserInputAttentionProjectionUnavailable,
  projectAuthorizedUserInputEvent,
  reconcileAuthorizedUserInputRequests,
} from "@/features/agents/userInputAttentionProjection";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import { buildChannelUserInputFilter } from "@/shared/api/relayChannelFilters";
import {
  beginExhaustiveAgentReceiptProjection,
  endExhaustiveAgentReceiptProjection,
  ingestAgentReceiptEvent,
  ingestAgentReceiptReviewEvent,
  isAgentReceiptProjectionUnavailable,
  markAgentReceiptProjectionUnavailable,
} from "@/features/agents/agentReceiptStore";
import {
  createHydrationRetryController,
  enumerateDurableActionEvents,
  isPermanentHydrationError,
  mergeDurableActionEvents,
} from "@/features/agents/durableActionHydration";
import type { RelayEvent } from "@/shared/api/types";
import { fetchRelayEventAncestry } from "@/features/agents/receiptParentLookup";

const LIVE_HOME_FEED_RETRY_BASE_MS = 1_000;
const LIVE_HOME_FEED_RETRY_MAX_MS = 30_000;
const DURABLE_ACTION_PAGE_SIZE = 500;
const RECEIPT_PARENT_BATCH_SIZE = 100;
const MAX_DURABLE_HYDRATION_BUFFER = 5_000;

type DurableProjectionFamily = "receipt" | "userInput";

class DurableProjectionHydrationError extends Error {
  constructor(
    readonly family: DurableProjectionFamily,
    readonly originalError: unknown,
  ) {
    super(
      originalError instanceof Error
        ? originalError.message
        : String(originalError),
    );
    this.name = "DurableProjectionHydrationError";
  }
}

function causalParentId(event: RelayEvent): string | null {
  const thread = getThreadReference(event.tags);
  return thread.parentId ?? thread.rootId;
}

export function useLiveHomeFeedActions(
  pubkey: string | undefined,
  onHomeFeedEvent: () => void,
  channelIds: readonly string[] = [],
) {
  const queryClient = useQueryClient();
  const ownedAgentPubkeys = useCurrentOwnedAgentPubkeys(pubkey);
  // Joined-string key: an unstable array identity from a caller can never
  // thrash the subscription lifecycle — only a real membership change
  // re-subscribes. The effect re-derives the array from this key.
  const channelIdsKey = channelIds.join(",");

  const handleLiveHomeFeedEvent = React.useEffectEvent(() => {
    onHomeFeedEvent();
  });
  const handleLiveReminderEvent = React.useEffectEvent(
    (normalizedPubkey: string) => {
      onHomeFeedEvent();
      void queryClient.invalidateQueries({
        queryKey: remindersQueryKey(normalizedPubkey),
      });
    },
  );

  React.useEffect(() => {
    const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";
    reconcileAuthorizedUserInputRequests(normalizedPubkey, ownedAgentPubkeys);
    if (!normalizedPubkey) {
      return;
    }
    const subscribedChannelIds =
      channelIdsKey.length > 0 ? channelIdsKey.split(",") : [];

    let isCancelled = false;
    const familyDisposers: Record<
      DurableProjectionFamily,
      Array<() => Promise<void>>
    > = {
      receipt: [],
      userInput: [],
    };
    let auxiliaryDisposers: Array<() => Promise<void>> = [];
    let retryTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
    let retryAttempt = 0;
    const since = Math.floor(Date.now() / 1_000);
    const familyHydrationReady: Record<DurableProjectionFamily, boolean> = {
      receipt: false,
      userInput: false,
    };
    const terminalFamilies = new Set<DurableProjectionFamily>();
    let durableProjectionGeneration = 0;
    let durableBufferOverflowGeneration = 0;
    const bufferedDurableEvents = new Map<string, RelayEvent>();
    const familyForEvent = (
      event: RelayEvent,
    ): DurableProjectionFamily | null => {
      if (
        [
          KIND_AGENT_USER_INPUT_REQUESTED,
          KIND_AGENT_USER_INPUT_ANSWER,
          KIND_AGENT_USER_INPUT_RESOLVED,
        ].includes(event.kind)
      ) {
        return "userInput";
      }
      if ([KIND_AGENT_RECEIPT, KIND_REACTION].includes(event.kind)) {
        return "receipt";
      }
      return null;
    };
    const isCoarselyAuthorizedDurableEvent = (event: RelayEvent) => {
      const author = event.pubkey.toLowerCase();
      return event.kind === KIND_REACTION
        ? author === normalizedPubkey
        : ownedAgentPubkeys.has(author);
    };
    const bufferDurableEvent = (event: RelayEvent) => {
      const family = familyForEvent(event);
      if (!family || terminalFamilies.has(family)) return false;
      if (!isCoarselyAuthorizedDurableEvent(event)) return false;
      if (
        !bufferedDurableEvents.has(event.id) &&
        [...bufferedDurableEvents.values()].filter(
          (candidate) => familyForEvent(candidate) === family,
        ).length >= MAX_DURABLE_HYDRATION_BUFFER
      ) {
        for (const [eventId, candidate] of bufferedDurableEvents) {
          if (familyForEvent(candidate) === family)
            bufferedDurableEvents.delete(eventId);
        }
        durableBufferOverflowGeneration += 1;
        durableProjectionGeneration += 1;
        familyHydrationReady[family] = false;
        if (family === "userInput")
          markUserInputAttentionProjectionUnavailable();
        else markAgentReceiptProjectionUnavailable();
        void hydrationRetry.run();
      }
      bufferedDurableEvents.set(event.id, event);
      return true;
    };

    const disposeAll = (currentDisposers: Array<() => Promise<void>>) => {
      void Promise.allSettled(currentDisposers.map((dispose) => dispose()));
    };
    const handleUserInputEvent = (
      event: RelayEvent,
      fallbackChannelId: string,
      knownParents?: ReadonlyMap<string, RelayEvent>,
    ) => {
      const parentId = causalParentId(event);
      projectAuthorizedUserInputEvent(
        event,
        fallbackChannelId,
        normalizedPubkey,
        ownedAgentPubkeys,
        knownParents?.get(parentId ?? ""),
        knownParents,
      );
    };
    const fetchCausalParents = async (events: readonly RelayEvent[]) => {
      const parentIds = [
        ...new Set(
          events
            .map((event) => causalParentId(event))
            .filter((eventId): eventId is string => Boolean(eventId)),
        ),
      ];
      return fetchRelayEventAncestry(
        (filter) => relayClient.fetchEvents(filter),
        parentIds,
        RECEIPT_PARENT_BATCH_SIZE,
      );
    };
    const handleReceiptEvent = async (
      event: RelayEvent,
      knownParents?: ReadonlyMap<string, RelayEvent>,
      shouldCommit: () => boolean = () => true,
    ) => {
      if (event.kind === KIND_AGENT_RECEIPT) {
        const parentId = causalParentId(event);
        const causalEvents =
          knownParents ?? (await fetchCausalParents([event]));
        const parent = causalEvents.get(parentId ?? "");
        if (!isCancelled && shouldCommit())
          ingestAgentReceiptEvent(event, parent, causalEvents);
      } else if (event.kind === KIND_REACTION) {
        if (!shouldCommit()) return;
        ingestAgentReceiptReviewEvent(
          event,
          normalizedPubkey,
          ownedAgentPubkeys,
        );
        if (isAgentReceiptProjectionUnavailable()) {
          throw new Error("agent receipt review projection capacity exceeded");
        }
      }
    };
    const hydrateDurableActions = async () => {
      durableProjectionGeneration += 1;
      const projectionGenerationAtStart = durableProjectionGeneration;
      const overflowGenerationAtStart = durableBufferOverflowGeneration;
      if (subscribedChannelIds.length === 0) return;
      const userInputActive =
        familyDisposers.userInput.length > 0 &&
        !terminalFamilies.has("userInput");
      const receiptActive =
        familyDisposers.receipt.length > 0 && !terminalFamilies.has("receipt");
      if (!userInputActive && !receiptActive) return;
      if (userInputActive) familyHydrationReady.userInput = false;
      if (receiptActive) familyHydrationReady.receipt = false;
      const enumerateFamily = async (
        family: DurableProjectionFamily,
        enumerate: () => Promise<RelayEvent[]>,
      ) => {
        try {
          return await enumerate();
        } catch (error) {
          throw new DurableProjectionHydrationError(family, error);
        }
      };
      const [userInputEvents, receiptEvents, reviewEvents] = await Promise.all([
        userInputActive
          ? enumerateFamily("userInput", () =>
              enumerateDurableActionEvents(
                (filter) => relayClient.fetchEvents(filter),
                {
                  kinds: [
                    KIND_AGENT_USER_INPUT_REQUESTED,
                    KIND_AGENT_USER_INPUT_ANSWER,
                    KIND_AGENT_USER_INPUT_RESOLVED,
                  ],
                  "#h": subscribedChannelIds,
                },
                DURABLE_ACTION_PAGE_SIZE,
              ),
            )
          : Promise.resolve([]),
        receiptActive
          ? enumerateFamily("receipt", () =>
              enumerateDurableActionEvents(
                (filter) => relayClient.fetchEvents(filter),
                {
                  kinds: [KIND_AGENT_RECEIPT],
                  "#h": subscribedChannelIds,
                },
                DURABLE_ACTION_PAGE_SIZE,
              ),
            )
          : Promise.resolve([]),
        receiptActive
          ? enumerateFamily("receipt", () =>
              enumerateDurableActionEvents(
                (filter) => relayClient.fetchEvents(filter),
                {
                  authors: [normalizedPubkey],
                  kinds: [KIND_REACTION],
                  "#h": subscribedChannelIds,
                },
                DURABLE_ACTION_PAGE_SIZE,
              ),
            )
          : Promise.resolve([]),
      ]);
      if (
        isCancelled ||
        durableProjectionGeneration !== projectionGenerationAtStart
      )
        return;
      let merged = mergeDurableActionEvents(
        userInputEvents,
        receiptEvents,
        reviewEvents,
        [],
      );
      let projectionStarted = false;
      for (;;) {
        const overlap = [...bufferedDurableEvents.values()];
        bufferedDurableEvents.clear();
        merged = mergeDurableActionEvents(
          merged.userInputEvents,
          merged.receiptEvents,
          merged.reviewEvents,
          overlap,
        );
        const causalParents = await fetchCausalParents([
          ...merged.userInputEvents.filter(
            (event) => event.kind === KIND_AGENT_USER_INPUT_REQUESTED,
          ),
          ...merged.receiptEvents,
        ]);
        if (
          isCancelled ||
          durableProjectionGeneration !== projectionGenerationAtStart
        )
          return;
        if (durableBufferOverflowGeneration !== overflowGenerationAtStart) {
          throw new Error(
            "Durable live-overlap capacity exceeded before projection commit",
          );
        }
        if (!projectionStarted) {
          if (userInputActive) beginExhaustiveUserInputProjection();
          if (receiptActive)
            beginExhaustiveAgentReceiptProjection(
              normalizedPubkey,
              ownedAgentPubkeys,
            );
          projectionStarted = true;
        }
        for (const event of merged.userInputEvents) {
          handleUserInputEvent(
            event,
            event.tags.find((tag) => tag[0] === "h")?.[1] ?? "",
            causalParents,
          );
        }
        // Receipts establish authority before reactions are projected, even if
        // relay pages or same-second ids arrive in the opposite order.
        for (const event of merged.receiptEvents) {
          await handleReceiptEvent(event, causalParents);
        }
        for (const event of merged.reviewEvents) {
          await handleReceiptEvent(event);
        }
        if (
          isCancelled ||
          durableProjectionGeneration !== projectionGenerationAtStart
        ) {
          if (
            userInputActive &&
            !terminalFamilies.has("userInput") &&
            !isUserInputAttentionProjectionUnavailable()
          ) {
            endExhaustiveUserInputProjection();
            familyHydrationReady.userInput = true;
          }
          if (
            receiptActive &&
            !terminalFamilies.has("receipt") &&
            !isAgentReceiptProjectionUnavailable()
          ) {
            endExhaustiveAgentReceiptProjection();
            familyHydrationReady.receipt = true;
          }
          return;
        }
        if (durableBufferOverflowGeneration !== overflowGenerationAtStart) {
          throw new Error(
            "Durable live-overlap capacity exceeded; exhaustive recovery required",
          );
        }
        if (bufferedDurableEvents.size === 0) {
          if (userInputActive) {
            endExhaustiveUserInputProjection();
            familyHydrationReady.userInput = true;
          }
          if (receiptActive) {
            endExhaustiveAgentReceiptProjection();
            familyHydrationReady.receipt = true;
          }
          break;
        }
      }
      handleLiveHomeFeedEvent();
    };
    const hydrateApprovals = async () => {
      beginExhaustiveApprovalProjection();
      let hydrationCompleted = false;
      let projectionReady = false;
      try {
        const [approvalRequestEvents, approvalTerminalEvents] =
          await Promise.all([
            enumerateDurableActionEvents(
              (filter) => relayClient.fetchEvents(filter),
              {
                kinds: [KIND_APPROVAL_REQUEST],
                "#p": [normalizedPubkey],
              },
              DURABLE_ACTION_PAGE_SIZE,
            ),
            enumerateDurableActionEvents(
              (filter) => relayClient.fetchEvents(filter),
              {
                authors: [normalizedPubkey],
                kinds: [KIND_APPROVAL_GRANT, KIND_APPROVAL_DENY],
              },
              DURABLE_ACTION_PAGE_SIZE,
            ),
          ]);
        if (isCancelled) return;
        for (const event of approvalRequestEvents)
          ingestApprovalRequestEvent(event);
        for (const event of approvalTerminalEvents) {
          await resolveApprovalRequestEvent(event);
        }
        hydrationCompleted = true;
      } finally {
        projectionReady = endExhaustiveApprovalProjection(hydrationCompleted);
      }
      if (!projectionReady) {
        throw new Error(
          "approval projection overflowed during exhaustive hydration",
        );
      }
      handleLiveHomeFeedEvent();
    };
    const markFamilyPermanent = (
      family: DurableProjectionFamily,
      error: unknown,
    ) => {
      if (terminalFamilies.has(family)) return;
      terminalFamilies.add(family);
      familyHydrationReady[family] = false;
      durableProjectionGeneration += 1;
      for (const [eventId, event] of bufferedDurableEvents) {
        if (familyForEvent(event) === family)
          bufferedDurableEvents.delete(eventId);
      }
      if (family === "userInput") {
        markUserInputAttentionProjectionUnavailable();
        endExhaustiveUserInputProjection();
      } else {
        markAgentReceiptProjectionUnavailable();
        endExhaustiveAgentReceiptProjection();
      }
      const currentDisposers = familyDisposers[family];
      familyDisposers[family] = [];
      disposeAll(currentDisposers);
      console.error(
        `Durable ${family} projection is unavailable until relay policy/configuration changes`,
        error,
      );
      if (
        (["receipt", "userInput"] as const).some(
          (candidate) =>
            !terminalFamilies.has(candidate) &&
            familyDisposers[candidate].length > 0,
        )
      ) {
        void hydrationRetry.run();
      }
      handleLiveHomeFeedEvent();
    };
    const hydrationRetry = createHydrationRetryController({
      hydrate: hydrateDurableActions,
      onError: (error) =>
        console.error(
          "Failed to hydrate durable agent attention events",
          error,
        ),
      onPermanentError: (error) => {
        if (error instanceof DurableProjectionHydrationError) {
          markFamilyPermanent(error.family, error.originalError);
          return;
        }
        for (const family of ["receipt", "userInput"] as const) {
          if (
            !terminalFamilies.has(family) &&
            familyDisposers[family].length > 0
          )
            markFamilyPermanent(family, error);
        }
      },
      retryDelayMs: LIVE_HOME_FEED_RETRY_MAX_MS,
      setTimeoutFn: (callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs),
      clearTimeoutFn: (timer) => globalThis.clearTimeout(timer),
      shouldRetry: (error) =>
        !isPermanentHydrationError(
          error instanceof DurableProjectionHydrationError
            ? error.originalError
            : error,
        ),
    });
    const approvalHydrationRetry = createHydrationRetryController({
      hydrate: hydrateApprovals,
      onError: (error) =>
        console.error("Failed to hydrate approval events", error),
      onPermanentError: (error) =>
        console.error(
          "Approval projection is unavailable until relay policy/configuration changes",
          error,
        ),
      retryDelayMs: LIVE_HOME_FEED_RETRY_MAX_MS,
      setTimeoutFn: (callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs),
      clearTimeoutFn: (timer) => globalThis.clearTimeout(timer),
    });
    const scheduleRetry = () => {
      if (isCancelled || retryTimer !== null) {
        return;
      }

      const delay = Math.min(
        LIVE_HOME_FEED_RETRY_MAX_MS,
        LIVE_HOME_FEED_RETRY_BASE_MS * 2 ** Math.min(retryAttempt, 5),
      );
      retryAttempt += 1;
      retryTimer = globalThis.setTimeout(() => {
        retryTimer = null;
        void startSubscriptions();
      }, delay);
    };
    const startSubscriptions = () => {
      if (isCancelled) {
        return Promise.resolve();
      }

      const subscribeCritical = (
        family: DurableProjectionFamily,
        filter: Parameters<typeof relayClient.subscribeLive>[0],
        onEvent: Parameters<typeof relayClient.subscribeLive>[1],
      ) =>
        relayClient.subscribeLive(filter, onEvent, (status) => {
          if (status.state === "closed") {
            markFamilyPermanent(family, new Error(status.message));
          }
        });
      const userInputSubscriptions =
        terminalFamilies.has("userInput") ||
        familyDisposers.userInput.length > 0
          ? []
          : subscribedChannelIds.map((channelId) =>
              subscribeCritical(
                "userInput",
                buildChannelUserInputFilter(channelId, 50, since),
                (event) => {
                  if (terminalFamilies.has("userInput")) return;
                  if (!bufferDurableEvent(event)) return;
                  if (!familyHydrationReady.userInput) {
                    return;
                  }
                  const eventGeneration = durableProjectionGeneration;
                  void fetchCausalParents([event])
                    .then((parents) => {
                      if (
                        isCancelled ||
                        !familyHydrationReady.userInput ||
                        durableProjectionGeneration !== eventGeneration
                      )
                        return;
                      handleUserInputEvent(event, channelId, parents);
                      if (isUserInputAttentionProjectionUnavailable()) {
                        bufferDurableEvent(event);
                        familyHydrationReady.userInput = false;
                        durableProjectionGeneration += 1;
                        void hydrationRetry.run();
                        return;
                      }
                      bufferedDurableEvents.delete(event.id);
                      handleLiveHomeFeedEvent();
                    })
                    .catch((error) => {
                      if (isCancelled) return;
                      bufferDurableEvent(event);
                      familyHydrationReady.userInput = false;
                      durableProjectionGeneration += 1;
                      console.error(
                        "Failed to validate user-input parent",
                        error,
                      );
                      void hydrationRetry.run();
                    });
                },
              ),
            );
      const receiptSubscriptions =
        terminalFamilies.has("receipt") || familyDisposers.receipt.length > 0
          ? []
          : subscribedChannelIds.flatMap((channelId) => [
              subscribeCritical(
                "receipt",
                {
                  kinds: [KIND_AGENT_RECEIPT],
                  "#h": [channelId],
                  limit: 0,
                  since,
                },
                (event) => {
                  if (terminalFamilies.has("receipt")) return;
                  if (!bufferDurableEvent(event)) return;
                  if (!familyHydrationReady.receipt) {
                    return;
                  }
                  const eventGeneration = durableProjectionGeneration;
                  const ownsGeneration = () =>
                    familyHydrationReady.receipt &&
                    durableProjectionGeneration === eventGeneration;
                  void handleReceiptEvent(event, undefined, ownsGeneration)
                    .then(() => {
                      if (!ownsGeneration()) return;
                      bufferedDurableEvents.delete(event.id);
                      handleLiveHomeFeedEvent();
                    })
                    .catch((error) => {
                      if (isCancelled) return;
                      bufferDurableEvent(event);
                      familyHydrationReady.receipt = false;
                      durableProjectionGeneration += 1;
                      console.error(
                        "Failed to validate agent receipt parent",
                        error,
                      );
                      void hydrationRetry.run();
                    });
                },
              ),
              subscribeCritical(
                "receipt",
                {
                  authors: [normalizedPubkey],
                  kinds: [KIND_REACTION],
                  "#h": [channelId],
                  limit: 0,
                  since,
                },
                (event) => {
                  if (terminalFamilies.has("receipt")) return;
                  if (!bufferDurableEvent(event)) return;
                  if (!familyHydrationReady.receipt) {
                    return;
                  }
                  const eventGeneration = durableProjectionGeneration;
                  const ownsGeneration = () =>
                    familyHydrationReady.receipt &&
                    durableProjectionGeneration === eventGeneration;
                  void handleReceiptEvent(event, undefined, ownsGeneration)
                    .then(() => {
                      if (!ownsGeneration()) return;
                      bufferedDurableEvents.delete(event.id);
                      handleLiveHomeFeedEvent();
                    })
                    .catch(() => {
                      if (isCancelled || terminalFamilies.has("receipt"))
                        return;
                      bufferDurableEvent(event);
                      familyHydrationReady.receipt = false;
                      durableProjectionGeneration += 1;
                      void hydrationRetry.run();
                    });
                },
              ),
            ]);

      const familySetups: Array<{
        family: DurableProjectionFamily;
        promises: Array<Promise<() => Promise<void>>>;
      }> = [];
      if (userInputSubscriptions.length > 0)
        familySetups.push({
          family: "userInput",
          promises: userInputSubscriptions,
        });
      if (receiptSubscriptions.length > 0)
        familySetups.push({
          family: "receipt",
          promises: receiptSubscriptions,
        });
      const subscribeAuxiliary = (
        label: string,
        filter: Parameters<typeof relayClient.subscribeLive>[0],
        onEvent: Parameters<typeof relayClient.subscribeLive>[1],
      ) =>
        relayClient.subscribeLive(filter, onEvent, (status) => {
          if (status.state !== "closed") return;
          console.error(`${label} subscription closed`, status.message);
          const currentDisposers = auxiliaryDisposers;
          auxiliaryDisposers = [];
          disposeAll(currentDisposers);
        });
      const auxiliarySubscriptions =
        auxiliaryDisposers.length > 0
          ? []
          : [
              subscribeAuxiliary(
                "Approval request",
                {
                  kinds: [KIND_APPROVAL_REQUEST],
                  "#p": [normalizedPubkey],
                  limit: 50,
                  since,
                },
                (event) => {
                  ingestApprovalRequestEvent(event);
                  handleLiveHomeFeedEvent();
                },
              ),
              subscribeAuxiliary(
                "Approval terminal",
                {
                  authors: [normalizedPubkey],
                  kinds: [KIND_APPROVAL_GRANT, KIND_APPROVAL_DENY],
                  limit: 50,
                  since,
                },
                (event) => {
                  // Refresh only after resolution settles so refetch cannot
                  // race the authoritative store update.
                  void resolveApprovalRequestEvent(event).finally(
                    handleLiveHomeFeedEvent,
                  );
                },
              ),
              subscribeAuxiliary(
                "Reminder",
                {
                  authors: [normalizedPubkey],
                  kinds: [KIND_EVENT_REMINDER],
                  limit: 50,
                  since,
                },
                () => {
                  handleLiveReminderEvent(normalizedPubkey);
                },
              ),
            ];

      return Promise.all([
        Promise.all(
          familySetups.map(async ({ family, promises }) => ({
            family,
            results: await Promise.allSettled(promises),
          })),
        ),
        Promise.allSettled(auxiliarySubscriptions),
      ]).then(([familyResults, auxiliaryResults]) => {
        let hydrationNeeded = false;
        let retryNeeded = false;
        for (const { family, results } of familyResults) {
          const fulfilled = results.flatMap((result) =>
            result.status === "fulfilled" ? [result.value] : [],
          );
          const rejected = results.filter(
            (result) => result.status === "rejected",
          );
          if (isCancelled) {
            disposeAll(fulfilled);
            continue;
          }
          if (terminalFamilies.has(family)) {
            disposeAll(fulfilled);
            continue;
          }
          for (const result of rejected) {
            console.error(
              `Failed to subscribe to ${family} live actions`,
              result.reason,
            );
          }
          const permanent = rejected.find((result) =>
            isPermanentHydrationError(result.reason),
          );
          if (permanent) {
            disposeAll(fulfilled);
            markFamilyPermanent(family, permanent.reason);
          } else if (rejected.length > 0 || fulfilled.length === 0) {
            disposeAll(fulfilled);
            retryNeeded = true;
          } else {
            familyDisposers[family] = fulfilled;
            hydrationNeeded = true;
          }
        }

        const fulfilledAuxiliary = auxiliaryResults.flatMap((result) =>
          result.status === "fulfilled" ? [result.value] : [],
        );
        const rejectedAuxiliary = auxiliaryResults.filter(
          (result) => result.status === "rejected",
        );
        if (isCancelled) {
          disposeAll(fulfilledAuxiliary);
          return;
        }
        for (const result of rejectedAuxiliary) {
          console.error(
            "Auxiliary home-feed subscription unavailable",
            result.reason,
          );
        }
        if (rejectedAuxiliary.length > 0) {
          disposeAll(fulfilledAuxiliary);
          if (
            !rejectedAuxiliary.some((result) =>
              isPermanentHydrationError(result.reason),
            )
          ) {
            retryNeeded = true;
          }
        } else if (fulfilledAuxiliary.length > 0) {
          auxiliaryDisposers = fulfilledAuxiliary;
          void approvalHydrationRetry.run();
        }

        if (retryNeeded) scheduleRetry();
        else retryAttempt = 0;
        if (hydrationNeeded) {
          // Install each live overlap before taking its history snapshot.
          void hydrationRetry.run();
        }
      });
    };

    void startSubscriptions();

    return () => {
      isCancelled = true;
      if (retryTimer !== null) {
        globalThis.clearTimeout(retryTimer);
      }
      hydrationRetry.stop();
      approvalHydrationRetry.stop();
      for (const family of ["receipt", "userInput"] as const) {
        const currentDisposers = familyDisposers[family];
        familyDisposers[family] = [];
        disposeAll(currentDisposers);
      }
      const currentAuxiliaryDisposers = auxiliaryDisposers;
      auxiliaryDisposers = [];
      disposeAll(currentAuxiliaryDisposers);
    };
  }, [channelIdsKey, ownedAgentPubkeys, pubkey]);
}
