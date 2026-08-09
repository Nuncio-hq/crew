import * as React from "react";
import { useIdentityQuery } from "@/shared/api/hooks";
import { relayClient } from "@/shared/api/relayClient";
import { buildChannelUserInputFilter } from "@/shared/api/relayChannelFilters";
import {
  createHydrationRetryController,
  enumerateDurableActionEvents,
} from "@/features/agents/durableActionHydration";
import type { RelayEvent } from "@/shared/api/types";
import { sendChannelUserInputAnswer } from "@/shared/api/tauriUserInput";
import { KIND_AGENT_USER_INPUT_REQUESTED } from "@/shared/constants/kinds";
import {
  buildSkippedAnswers,
  buildUserInputAnswers,
  deriveAnsweredUserInputs,
  deriveResolvedUserInputs,
  derivePendingUserInputs,
  publishUserInputAnswer,
  type UserInputAnswers,
  type UserInputEvent,
} from "@/features/channels/lib/userInput";
import {
  projectAuthorizedUserInputEvent,
  reconcileAuthorizedUserInputRequests,
  type AuthorizedUserInputRequest,
  validateAuthorizedUserInputRequest,
  validateAuthorizedUserInputTransition,
} from "@/features/agents/userInputAttentionProjection";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";

const USER_INPUT_PAGE_SIZE = 200;
const USER_INPUT_HYDRATION_RETRY_MS = 5_000;

export function useChannelUserInput(channelId: string | null) {
  const identityQuery = useIdentityQuery();
  const currentPubkey = identityQuery.data?.pubkey ?? "";
  const ownedAgentPubkeys = useCurrentOwnedAgentPubkeys(currentPubkey);
  const [events, setEvents] = React.useState<RelayEvent[]>([]);
  const [optimisticallyResolved, setOptimisticallyResolved] = React.useState(
    () => new Set<string>(),
  );
  const [sendingRequestId, setSendingRequestId] = React.useState<string | null>(
    null,
  );
  const [sentRequestIds, setSentRequestIds] = React.useState(
    () => new Set<string>(),
  );
  const [errors, setErrors] = React.useState<Record<string, string>>({});
  const [visibleRequestIds, setVisibleRequestIds] = React.useState(
    () => new Set<string>(),
  );
  const [dismissedResolutionIds, setDismissedResolutionIds] = React.useState(
    () => new Set<string>(),
  );

  React.useEffect(() => {
    reconcileAuthorizedUserInputRequests(currentPubkey, ownedAgentPubkeys);
    setEvents([]);
    setOptimisticallyResolved(new Set());
    setSentRequestIds(new Set());
    setErrors({});
    setVisibleRequestIds(new Set());
    setDismissedResolutionIds(new Set());
    if (!channelId) return;

    let cancelled = false;
    let dispose: (() => Promise<void>) | undefined;
    const authorizedRequests = new Map<string, AuthorizedUserInputRequest>();
    const candidates = new Map<string, RelayEvent>();
    const liveFilter = buildChannelUserInputFilter(channelId, 0);
    const { limit: _liveLimit, ...historyFilter } = liveFilter;
    const authorizeEvent = (event: RelayEvent) => {
      const request = validateAuthorizedUserInputRequest(
        event,
        currentPubkey,
        ownedAgentPubkeys,
      );
      if (request) {
        authorizedRequests.set(request.id, request);
        projectAuthorizedUserInputEvent(
          event,
          channelId,
          currentPubkey,
          ownedAgentPubkeys,
        );
        return true;
      }
      const eTags = event.tags.filter((tag) => tag[0] === "e");
      const target =
        eTags.length === 1 ? authorizedRequests.get(eTags[0]?.[1] ?? "") : null;
      if (
        !target ||
        !validateAuthorizedUserInputTransition(
          event,
          target,
          currentPubkey,
          ownedAgentPubkeys,
        )
      ) {
        return false;
      }
      projectAuthorizedUserInputEvent(
        event,
        channelId,
        currentPubkey,
        ownedAgentPubkeys,
      );
      return true;
    };
    const rebuildAuthorizedEvents = (compactHistory = false) => {
      authorizedRequests.clear();
      const ordered = [...candidates.values()].sort(
        (left, right) =>
          Number(right.kind === KIND_AGENT_USER_INPUT_REQUESTED) -
            Number(left.kind === KIND_AGENT_USER_INPUT_REQUESTED) ||
          left.created_at - right.created_at ||
          left.id.localeCompare(right.id),
      );
      let authorized = ordered.filter(authorizeEvent);
      const activeIds = new Set([
        ...derivePendingUserInputs(authorized, currentPubkey).map(
          ({ event }) => event.id,
        ),
        ...deriveAnsweredUserInputs(authorized, currentPubkey).map(
          ({ event }) => event.id,
        ),
      ]);
      if (compactHistory) {
        authorized = authorized.filter((event) => {
          if (activeIds.has(event.id)) return true;
          const eTags = event.tags.filter((tag) => tag[0] === "e");
          return eTags.length === 1 && activeIds.has(eTags[0]?.[1] ?? "");
        });
        candidates.clear();
        for (const event of authorized) candidates.set(event.id, event);
        for (const id of authorizedRequests.keys()) {
          if (!activeIds.has(id)) authorizedRequests.delete(id);
        }
      }
      setVisibleRequestIds((current) => {
        const next = new Set(current);
        for (const id of activeIds) next.add(id);
        return next;
      });
      setEvents(
        authorized.sort(
          (left, right) =>
            right.created_at - left.created_at ||
            right.id.localeCompare(left.id),
        ),
      );
    };
    const onEvent = (event: RelayEvent) => {
      if (cancelled || candidates.has(event.id)) return;
      candidates.set(event.id, event);
      if (event.kind === KIND_AGENT_USER_INPUT_REQUESTED) {
        setVisibleRequestIds((current) => {
          if (current.has(event.id)) return current;
          return new Set(current).add(event.id);
        });
      }
      rebuildAuthorizedEvents();
    };

    const hydrate = async () => {
      if (!dispose) {
        dispose = await relayClient.subscribeLive(liveFilter, onEvent);
        if (cancelled) {
          await dispose();
          dispose = undefined;
          return;
        }
      }
      const history = await enumerateDurableActionEvents(
        (pageFilter) => relayClient.fetchEvents(pageFilter),
        historyFilter,
        USER_INPUT_PAGE_SIZE,
      );
      if (!cancelled) {
        for (const event of history) candidates.set(event.id, event);
        rebuildAuthorizedEvents(true);
      }
    };
    const hydrationRetry = createHydrationRetryController({
      hydrate,
      onError: (error) =>
        console.error("Failed to load agent questions", error),
      retryDelayMs: USER_INPUT_HYDRATION_RETRY_MS,
      setTimeoutFn: (callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs),
      clearTimeoutFn: (timer) => globalThis.clearTimeout(timer),
    });

    void hydrationRetry.run();
    return () => {
      cancelled = true;
      hydrationRetry.stop();
      void dispose?.();
    };
  }, [channelId, currentPubkey, ownedAgentPubkeys]);

  const pending = React.useMemo(
    () =>
      derivePendingUserInputs(events, currentPubkey, optimisticallyResolved),
    [currentPubkey, events, optimisticallyResolved],
  );
  const sent = React.useMemo(() => {
    const byId = new Map(
      deriveAnsweredUserInputs(events, currentPubkey).map((item) => [
        item.event.id,
        item,
      ]),
    );
    for (const item of derivePendingUserInputs(events, currentPubkey)) {
      if (sentRequestIds.has(item.event.id)) byId.set(item.event.id, item);
    }
    return [...byId.values()].sort(
      (left, right) => right.event.created_at - left.event.created_at,
    );
  }, [currentPubkey, events, sentRequestIds]);
  const resolved = React.useMemo(
    () =>
      deriveResolvedUserInputs(events).filter(
        ({ event }) =>
          visibleRequestIds.has(event.id) &&
          !dismissedResolutionIds.has(event.id),
      ),
    [dismissedResolutionIds, events, visibleRequestIds],
  );
  const dismissResolved = React.useCallback((requestEventId: string) => {
    setDismissedResolutionIds((current) => {
      const next = new Set(current);
      next.add(requestEventId);
      return next;
    });
  }, []);

  const answer = React.useCallback(
    async (request: UserInputEvent, answers: UserInputAnswers) => {
      setErrors((current) => {
        const next = { ...current };
        delete next[request.event.id];
        return next;
      });
      setSendingRequestId(request.event.id);
      try {
        const error = await publishUserInputAnswer(
          sendChannelUserInputAnswer,
          channelId ?? request.request.channel_id,
          request.event.id,
          buildUserInputAnswers(answers),
        );
        if (error) {
          setErrors((current) => ({ ...current, [request.event.id]: error }));
          return;
        }
        setOptimisticallyResolved((current) => {
          const next = new Set(current);
          next.add(request.event.id);
          return next;
        });
        setSentRequestIds((current) => {
          const next = new Set(current);
          next.add(request.event.id);
          return next;
        });
      } finally {
        setSendingRequestId(null);
      }
    },
    [channelId],
  );

  const skip = React.useCallback(
    (request: UserInputEvent) =>
      answer(
        request,
        buildSkippedAnswers(request.request.questions.map((q) => q.id)),
      ),
    [answer],
  );

  return {
    hasCards: pending.length > 0 || sent.length > 0 || resolved.length > 0,
    pending,
    sent,
    resolved,
    currentPubkey,
    sendingRequestId,
    sentRequestIds,
    errors,
    answer,
    skip,
    dismissResolved,
    isLoading: Boolean(channelId) && identityQuery.isLoading,
  };
}
