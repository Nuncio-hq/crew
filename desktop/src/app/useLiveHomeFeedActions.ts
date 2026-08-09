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
  projectAuthorizedUserInputEvent,
  reconcileAuthorizedUserInputRequests,
} from "@/features/agents/userInputAttentionProjection";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import { buildChannelUserInputFilter } from "@/shared/api/relayChannelFilters";
import {
  ingestAgentReceiptEvent,
  ingestAgentReceiptReviewEvent,
} from "@/features/agents/agentReceiptStore";
import {
  createHydrationRetryController,
  enumerateDurableActionEvents,
  mergeDurableActionEvents,
} from "@/features/agents/durableActionHydration";
import type { RelayEvent } from "@/shared/api/types";
import { buildReceiptParentFilter } from "@/features/agents/receiptParentLookup";

const LIVE_HOME_FEED_RETRY_BASE_MS = 1_000;
const LIVE_HOME_FEED_RETRY_MAX_MS = 30_000;
const DURABLE_ACTION_PAGE_SIZE = 500;
const RECEIPT_PARENT_BATCH_SIZE = 100;

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
    const bufferedDurableEvents = new Map<string, RelayEvent>();

    const disposeAll = (currentDisposers: Array<() => Promise<void>>) => {
      void Promise.allSettled(currentDisposers.map((dispose) => dispose()));
    };
    const handleUserInputEvent = (
      event: RelayEvent,
      fallbackChannelId: string,
    ) => {
      projectAuthorizedUserInputEvent(
        event,
        fallbackChannelId,
        normalizedPubkey,
        ownedAgentPubkeys,
      );
    };
    const fetchReceiptParents = async (events: readonly RelayEvent[]) => {
      const parentIds = [
        ...new Set(
          events
            .map((event) => getThreadReference(event.tags).parentId)
            .filter((eventId): eventId is string => Boolean(eventId)),
        ),
      ];
      const parentsById = new Map<string, RelayEvent>();
      for (
        let offset = 0;
        offset < parentIds.length;
        offset += RECEIPT_PARENT_BATCH_SIZE
      ) {
        const ids = parentIds.slice(offset, offset + RECEIPT_PARENT_BATCH_SIZE);
        const parents = await relayClient.fetchEvents(
          buildReceiptParentFilter(ids),
        );
        for (const parent of parents) parentsById.set(parent.id, parent);
      }
      return parentsById;
    };
    const handleReceiptEvent = async (
      event: RelayEvent,
      knownParents?: ReadonlyMap<string, RelayEvent>,
    ) => {
      if (event.kind === KIND_AGENT_RECEIPT) {
        const parentId = getThreadReference(event.tags).parentId;
        const parent = knownParents
          ? knownParents.get(parentId ?? "")
          : (await fetchReceiptParents([event])).get(parentId ?? "");
        if (!isCancelled) ingestAgentReceiptEvent(event, parent);
      } else if (event.kind === KIND_REACTION) {
        ingestAgentReceiptReviewEvent(
          event,
          normalizedPubkey,
          ownedAgentPubkeys,
        );
      }
    };
    const hydrateDurableActions = async () => {
      if (subscribedChannelIds.length === 0) return;
      const [userInputEvents, receiptEvents, reviewEvents] = await Promise.all([
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
      ]);
      if (isCancelled) return;
      const merged = mergeDurableActionEvents(
        userInputEvents,
        receiptEvents,
        reviewEvents,
        [...bufferedDurableEvents.values()],
      );
      const receiptParents = await fetchReceiptParents(merged.receiptEvents);
      if (isCancelled) return;
      bufferedDurableEvents.clear();
      durableHydrationReady = true;
      for (const event of merged.userInputEvents) {
        handleUserInputEvent(
          event,
          event.tags.find((tag) => tag[0] === "h")?.[1] ?? "",
        );
      }
      // Receipts establish authority before reactions are projected, even if
      // relay pages or same-second ids arrive in the opposite order.
      for (const event of merged.receiptEvents) {
        await handleReceiptEvent(event, receiptParents);
      }
      for (const event of merged.reviewEvents) {
        await handleReceiptEvent(event);
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
      retryDelayMs: LIVE_HOME_FEED_RETRY_MAX_MS,
      setTimeoutFn: (callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs),
      clearTimeoutFn: (timer) => globalThis.clearTimeout(timer),
    });
    const scheduleRetry = () => {
      if (isCancelled) {
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
      if (isCancelled) {
        return;
      }

      const userInputSubscriptions = subscribedChannelIds.map((channelId) =>
        relayClient.subscribeLive(
          buildChannelUserInputFilter(channelId, 50, since),
          (event) => {
            if (!durableHydrationReady) {
              bufferedDurableEvents.set(event.id, event);
              return;
            }
            handleUserInputEvent(event, channelId);
            handleLiveHomeFeedEvent();
          },
        ),
      );
      const receiptSubscriptions = subscribedChannelIds.flatMap((channelId) => [
        relayClient.subscribeLive(
          {
            kinds: [KIND_AGENT_RECEIPT],
            "#h": [channelId],
            limit: 0,
            since,
          },
          (event) => {
            if (!durableHydrationReady) {
              bufferedDurableEvents.set(event.id, event);
              return;
            }
            void handleReceiptEvent(event)
              .then(() => handleLiveHomeFeedEvent())
              .catch((error) => {
                if (isCancelled) return;
                bufferedDurableEvents.set(event.id, event);
                durableHydrationReady = false;
                console.error("Failed to validate agent receipt parent", error);
                void hydrationRetry.run();
              });
          },
        ),
        relayClient.subscribeLive(
          {
            authors: [normalizedPubkey],
            kinds: [KIND_REACTION],
            "#h": [channelId],
            limit: 0,
            since,
          },
          (event) => {
            if (!durableHydrationReady) {
              bufferedDurableEvents.set(event.id, event);
              return;
            }
            void handleReceiptEvent(event).then(() =>
              handleLiveHomeFeedEvent(),
            );
          },
        ),
      ]);

      void Promise.allSettled([
        ...userInputSubscriptions,
        ...receiptSubscriptions,
        relayClient.subscribeLive(
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
        relayClient.subscribeLive(
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
        relayClient.subscribeLive(
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

        if (rejectedResults.length > 0 || nextDisposers.length === 0) {
          disposeAll(nextDisposers);
          scheduleRetry();
          return;
        }

        retryAttempt = 0;
        disposers = nextDisposers;
      });
    };

    void hydrationRetry.run();
    startSubscriptions();

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
