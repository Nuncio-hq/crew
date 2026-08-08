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
import {
  ingestApprovalRequestEvent,
  resolveApprovalRequestEvent,
  ingestUserInputRequest,
  resolveUserInputRequest,
} from "@/features/agents/needsYouStore";
import {
  deriveUserInputRootEventId,
  getAnswerRequestId,
  getResolvedRequestId,
  parseUserInputRequest,
} from "@/features/channels/lib/userInput";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import { buildChannelUserInputFilter } from "@/shared/api/relayChannelFilters";
import {
  getAgentReceipts,
  ingestAgentReceiptEvent,
  ingestAgentReceiptReviewEvent,
} from "@/features/agents/agentReceiptStore";
import type { RelayEvent } from "@/shared/api/types";

const LIVE_HOME_FEED_RETRY_BASE_MS = 1_000;
const LIVE_HOME_FEED_RETRY_MAX_MS = 30_000;

export function useLiveHomeFeedActions(
  pubkey: string | undefined,
  onHomeFeedEvent: () => void,
  channelIds: readonly string[] = [],
) {
  const queryClient = useQueryClient();
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

    const disposeAll = (currentDisposers: Array<() => Promise<void>>) => {
      void Promise.allSettled(currentDisposers.map((dispose) => dispose()));
    };
    const handleUserInputEvent = (
      event: RelayEvent,
      fallbackChannelId: string,
    ) => {
      const request = parseUserInputRequest(event);
      if (request) {
        const resolvedChannelId = request.channel_id || fallbackChannelId;
        const rootEventId = deriveUserInputRootEventId(event);
        const conversationId = deriveAgentConversationIdOrNull(
          resolvedChannelId,
          rootEventId,
        );
        if (conversationId) {
          ingestUserInputRequest({
            id: event.id,
            channelId: resolvedChannelId,
            rootEventId,
            conversationId,
            agentPubkey: event.pubkey,
            createdAt: event.created_at * 1_000,
          });
        }
        return;
      }
      const requestId =
        getAnswerRequestId(event) ?? getResolvedRequestId(event);
      if (requestId) resolveUserInputRequest(requestId);
    };
    const handleReceiptEvent = (event: RelayEvent) => {
      if (event.kind === KIND_AGENT_RECEIPT) {
        ingestAgentReceiptEvent(event);
      } else if (event.kind === KIND_REACTION) {
        ingestAgentReceiptReviewEvent(event, normalizedPubkey);
      }
    };
    const hydrateDurableActions = async () => {
      if (subscribedChannelIds.length === 0) return;
      const [userInputEvents, receiptEvents] = await Promise.all([
        relayClient.fetchEvents({
          kinds: [
            KIND_AGENT_USER_INPUT_REQUESTED,
            KIND_AGENT_USER_INPUT_ANSWER,
            KIND_AGENT_USER_INPUT_RESOLVED,
          ],
          "#h": subscribedChannelIds,
          limit: 1_000,
        }),
        relayClient.fetchEvents({
          kinds: [KIND_AGENT_RECEIPT],
          "#h": subscribedChannelIds,
          limit: 500,
        }),
      ]);
      for (const event of userInputEvents.sort(
        (a, b) => a.created_at - b.created_at,
      )) {
        handleUserInputEvent(
          event,
          event.tags.find((tag) => tag[0] === "h")?.[1] ?? "",
        );
      }
      for (const event of receiptEvents) ingestAgentReceiptEvent(event);

      const receiptIds = getAgentReceipts().map((receipt) => receipt.id);
      if (receiptIds.length > 0) {
        const reviewEvents = await relayClient.fetchEvents({
          authors: [normalizedPubkey],
          kinds: [KIND_REACTION],
          "#e": receiptIds,
          limit: Math.min(1_000, receiptIds.length * 4),
        });
        for (const event of reviewEvents) {
          ingestAgentReceiptReviewEvent(event, normalizedPubkey);
        }
      }
      handleLiveHomeFeedEvent();
    };
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
            handleUserInputEvent(event, channelId);
            handleLiveHomeFeedEvent();
          },
        ),
      );
      const receiptSubscriptions = subscribedChannelIds.map((channelId) =>
        relayClient.subscribeLive(
          {
            kinds: [KIND_AGENT_RECEIPT],
            "#h": [channelId],
            limit: 0,
            since,
          },
          (event) => {
            handleReceiptEvent(event);
            handleLiveHomeFeedEvent();
          },
        ),
      );

      void Promise.allSettled([
        ...userInputSubscriptions,
        ...receiptSubscriptions,
        // NIP-25 reactions reference their target with `e`; they do not carry
        // the target's channel `h` tag. Subscribe narrowly to the owner's
        // reactions and let the receipt store accept only known receipt ids.
        relayClient.subscribeLive(
          {
            authors: [normalizedPubkey],
            kinds: [KIND_REACTION],
            limit: 0,
            since,
          },
          (event) => {
            handleReceiptEvent(event);
            handleLiveHomeFeedEvent();
          },
        ),
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

    void hydrateDurableActions().catch((error) => {
      console.error("Failed to hydrate durable agent attention events", error);
    });
    startSubscriptions();

    return () => {
      isCancelled = true;
      if (retryTimer !== null) {
        globalThis.clearTimeout(retryTimer);
      }
      const currentDisposers = disposers;
      disposers = [];
      disposeAll(currentDisposers);
    };
  }, [channelIdsKey, pubkey]);
}
