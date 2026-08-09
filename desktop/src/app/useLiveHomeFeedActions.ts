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
    let disposers: Array<() => Promise<void>> = [];
    let retryTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
    let retryAttempt = 0;
    const since = Math.floor(Date.now() / 1_000);
    let durableHydrationReady = false;
    let durableHydrationTerminal = false;
    let durableProjectionGeneration = 0;
    let durableBufferOverflowGeneration = 0;
    const bufferedDurableEvents = new Map<string, RelayEvent>();
    const isCoarselyAuthorizedDurableEvent = (event: RelayEvent) => {
      const author = event.pubkey.toLowerCase();
      return event.kind === KIND_REACTION
        ? author === normalizedPubkey
        : ownedAgentPubkeys.has(author);
    };
    const bufferDurableEvent = (event: RelayEvent) => {
      if (!isCoarselyAuthorizedDurableEvent(event)) return false;
      if (
        !bufferedDurableEvents.has(event.id) &&
        bufferedDurableEvents.size >= MAX_DURABLE_HYDRATION_BUFFER
      ) {
        bufferedDurableEvents.clear();
        durableBufferOverflowGeneration += 1;
        durableProjectionGeneration += 1;
        durableHydrationReady = false;
        markUserInputAttentionProjectionUnavailable();
        markAgentReceiptProjectionUnavailable();
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
      durableHydrationReady = false;
      durableProjectionGeneration += 1;
      const overflowGenerationAtStart = durableBufferOverflowGeneration;
      if (subscribedChannelIds.length === 0) return;
      const [
        userInputEvents,
        receiptEvents,
        reviewEvents,
        approvalRequestEvents,
        approvalTerminalEvents,
      ] = await Promise.all([
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
        enumerateDurableActionEvents(
          (filter) => relayClient.fetchEvents(filter),
          {
            kinds: [KIND_AGENT_RECEIPT],
            "#h": subscribedChannelIds,
          },
          DURABLE_ACTION_PAGE_SIZE,
        ),
        enumerateDurableActionEvents(
          (filter) => relayClient.fetchEvents(filter),
          {
            authors: [normalizedPubkey],
            kinds: [KIND_REACTION],
            "#h": subscribedChannelIds,
          },
          DURABLE_ACTION_PAGE_SIZE,
        ),
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
        if (isCancelled) return;
        if (durableBufferOverflowGeneration !== overflowGenerationAtStart) {
          throw new Error(
            "Durable live-overlap capacity exceeded before projection commit",
          );
        }
        if (!projectionStarted) {
          beginExhaustiveUserInputProjection();
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
        if (durableBufferOverflowGeneration !== overflowGenerationAtStart) {
          throw new Error(
            "Durable live-overlap capacity exceeded; exhaustive recovery required",
          );
        }
        if (bufferedDurableEvents.size === 0) {
          endExhaustiveUserInputProjection();
          endExhaustiveAgentReceiptProjection();
          durableHydrationReady = true;
          break;
        }
      }
      handleLiveHomeFeedEvent();
    };
    const markPermanentHydrationFailure = (error: unknown) => {
      durableHydrationTerminal = true;
      durableHydrationReady = false;
      markUserInputAttentionProjectionUnavailable();
      markAgentReceiptProjectionUnavailable();
      endExhaustiveUserInputProjection();
      endExhaustiveAgentReceiptProjection();
      bufferedDurableEvents.clear();
      const currentDisposers = disposers;
      disposers = [];
      disposeAll(currentDisposers);
      console.error(
        "Durable agent attention projection is unavailable until relay policy/configuration changes",
        error,
      );
      handleLiveHomeFeedEvent();
    };
    const hydrationRetry = createHydrationRetryController({
      hydrate: hydrateDurableActions,
      onError: (error) =>
        console.error(
          "Failed to hydrate durable agent attention events",
          error,
        ),
      onPermanentError: markPermanentHydrationFailure,
      retryDelayMs: LIVE_HOME_FEED_RETRY_MAX_MS,
      setTimeoutFn: (callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs),
      clearTimeoutFn: (timer) => globalThis.clearTimeout(timer),
    });
    const scheduleRetry = () => {
      if (isCancelled || durableHydrationTerminal) {
        return;
      }

      const delay = Math.min(
        LIVE_HOME_FEED_RETRY_MAX_MS,
        LIVE_HOME_FEED_RETRY_BASE_MS * 2 ** Math.min(retryAttempt, 5),
      );
      retryAttempt += 1;
      retryTimer = globalThis.setTimeout(startSubscriptions, delay);
    };
    const startSubscriptions = () => {
      if (isCancelled || durableHydrationTerminal) {
        return Promise.resolve();
      }

      const subscribeCritical = (
        filter: Parameters<typeof relayClient.subscribeLive>[0],
        onEvent: Parameters<typeof relayClient.subscribeLive>[1],
      ) =>
        relayClient.subscribeLive(filter, onEvent, (status) => {
          if (status.state === "closed") {
            markPermanentHydrationFailure(new Error(status.message));
          }
        });
      const userInputSubscriptions = subscribedChannelIds.map((channelId) =>
        subscribeCritical(
          buildChannelUserInputFilter(channelId, 50, since),
          (event) => {
            if (durableHydrationTerminal) return;
            if (!bufferDurableEvent(event)) return;
            if (!durableHydrationReady) {
              return;
            }
            const eventGeneration = durableProjectionGeneration;
            void fetchCausalParents([event])
              .then((parents) => {
                if (
                  isCancelled ||
                  !durableHydrationReady ||
                  durableProjectionGeneration !== eventGeneration
                )
                  return;
                handleUserInputEvent(event, channelId, parents);
                if (isUserInputAttentionProjectionUnavailable()) {
                  bufferDurableEvent(event);
                  durableHydrationReady = false;
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
                durableHydrationReady = false;
                durableProjectionGeneration += 1;
                console.error("Failed to validate user-input parent", error);
                void hydrationRetry.run();
              });
          },
        ),
      );
      const receiptSubscriptions = subscribedChannelIds.flatMap((channelId) => [
        subscribeCritical(
          {
            kinds: [KIND_AGENT_RECEIPT],
            "#h": [channelId],
            limit: 0,
            since,
          },
          (event) => {
            if (durableHydrationTerminal) return;
            if (!bufferDurableEvent(event)) return;
            if (!durableHydrationReady) {
              return;
            }
            const eventGeneration = durableProjectionGeneration;
            const ownsGeneration = () =>
              durableHydrationReady &&
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
                durableHydrationReady = false;
                durableProjectionGeneration += 1;
                console.error("Failed to validate agent receipt parent", error);
                void hydrationRetry.run();
              });
          },
        ),
        subscribeCritical(
          {
            authors: [normalizedPubkey],
            kinds: [KIND_REACTION],
            "#h": [channelId],
            limit: 0,
            since,
          },
          (event) => {
            if (durableHydrationTerminal) return;
            if (!bufferDurableEvent(event)) return;
            if (!durableHydrationReady) {
              return;
            }
            const eventGeneration = durableProjectionGeneration;
            const ownsGeneration = () =>
              durableHydrationReady &&
              durableProjectionGeneration === eventGeneration;
            void handleReceiptEvent(event, undefined, ownsGeneration)
              .then(() => {
                if (!ownsGeneration()) return;
                bufferedDurableEvents.delete(event.id);
                handleLiveHomeFeedEvent();
              })
              .catch(() => {
                if (isCancelled || durableHydrationTerminal) return;
                bufferDurableEvent(event);
                durableHydrationReady = false;
                durableProjectionGeneration += 1;
                void hydrationRetry.run();
              });
          },
        ),
      ]);

      return Promise.allSettled([
        ...userInputSubscriptions,
        ...receiptSubscriptions,
        subscribeCritical(
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
        subscribeCritical(
          {
            authors: [normalizedPubkey],
            kinds: [KIND_APPROVAL_GRANT, KIND_APPROVAL_DENY],
            limit: 50,
            since,
          },
          (event) => {
            // Refresh the feed only after the resolution settles so the
            // refetched needs_action can't race the store update.
            void resolveApprovalRequestEvent(event).finally(
              handleLiveHomeFeedEvent,
            );
          },
        ),
        relayClient
          .subscribeLive(
            {
              authors: [normalizedPubkey],
              kinds: [KIND_EVENT_REMINDER],
              limit: 50,
              since,
            },
            () => {
              handleLiveReminderEvent(normalizedPubkey);
            },
          )
          .catch((error) => {
            console.error("Optional reminder subscription unavailable", error);
            return async () => {};
          }),
      ]).then((results) => {
        const nextDisposers = results.flatMap((result) =>
          result.status === "fulfilled" ? [result.value] : [],
        );
        const rejectedResults = results.filter(
          (result) => result.status === "rejected",
        );
        for (const result of rejectedResults) {
          console.error(
            "Failed to subscribe to live home feed actions; retrying",
            result.reason,
          );
        }

        if (isCancelled) {
          disposeAll(nextDisposers);
          return;
        }

        const permanentRejection = rejectedResults.find((result) =>
          isPermanentHydrationError(result.reason),
        );
        if (permanentRejection) {
          disposeAll(nextDisposers);
          markPermanentHydrationFailure(permanentRejection.reason);
          return;
        }

        if (rejectedResults.length > 0 || nextDisposers.length === 0) {
          disposeAll(nextDisposers);
          scheduleRetry();
          return;
        }

        retryAttempt = 0;
        disposers = nextDisposers;
        // Install the live overlap before taking the history snapshot. Durable
        // events arriving during hydration are buffered and merged below.
        void hydrationRetry.run();
      });
    };

    void startSubscriptions();

    return () => {
      isCancelled = true;
      if (retryTimer !== null) {
        globalThis.clearTimeout(retryTimer);
      }
      hydrationRetry.stop();
      const currentDisposers = disposers;
      disposers = [];
      disposeAll(currentDisposers);
    };
  }, [channelIdsKey, ownedAgentPubkeys, pubkey]);
}
