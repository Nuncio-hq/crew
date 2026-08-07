import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { remindersQueryKey } from "@/features/reminders/hooks";
import { relayClient } from "@/shared/api/relayClient";
import {
  KIND_APPROVAL_REQUEST,
  KIND_APPROVAL_GRANT,
  KIND_APPROVAL_DENY,
  KIND_EVENT_REMINDER,
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
            const request = parseUserInputRequest(event);
            if (request) {
              const resolvedChannelId = request.channel_id || channelId;
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
            } else {
              const requestId =
                getAnswerRequestId(event) ?? getResolvedRequestId(event);
              if (requestId) resolveUserInputRequest(requestId);
            }
            handleLiveHomeFeedEvent();
          },
        ),
      );

      void Promise.allSettled([
        ...userInputSubscriptions,
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
