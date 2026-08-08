import * as React from "react";
import { useIdentityQuery } from "@/shared/api/hooks";
import { relayClient } from "@/shared/api/relayClient";
import { buildChannelUserInputFilter } from "@/shared/api/relayChannelFilters";
import type { RelayEvent } from "@/shared/api/types";
import { sendChannelUserInputAnswer } from "@/shared/api/tauriUserInput";
import { KIND_AGENT_USER_INPUT_REQUESTED } from "@/shared/constants/kinds";
import {
  buildSkippedAnswers,
  buildUserInputAnswers,
  deriveResolvedUserInputs,
  derivePendingUserInputs,
  publishUserInputAnswer,
  type UserInputAnswers,
  type UserInputEvent,
} from "@/features/channels/lib/userInput";
import {
  projectAuthorizedUserInputEvent,
  type AuthorizedUserInputRequest,
  validateAuthorizedUserInputRequest,
  validateAuthorizedUserInputTransition,
} from "@/features/agents/userInputAttentionProjection";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";

const RETAINED_EVENTS = 200;

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
    const filter = buildChannelUserInputFilter(channelId, RETAINED_EVENTS);
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
    const onEvent = (event: RelayEvent) => {
      if (cancelled || !authorizeEvent(event)) return;
      if (event.kind === KIND_AGENT_USER_INPUT_REQUESTED) {
        setVisibleRequestIds((current) => {
          if (current.has(event.id)) return current;
          return new Set(current).add(event.id);
        });
      }
      setEvents((current) => {
        if (current.some((existing) => existing.id === event.id))
          return current;
        return [event, ...current].slice(0, RETAINED_EVENTS);
      });
    };

    const load = async () => {
      try {
        dispose = await relayClient.subscribeLive(filter, onEvent);
        if (cancelled) {
          await dispose();
          dispose = undefined;
          return;
        }
        const history = await relayClient.fetchEvents(filter);
        if (!cancelled) {
          const ordered = [...history].sort(
            (left, right) =>
              Number(right.kind === KIND_AGENT_USER_INPUT_REQUESTED) -
                Number(left.kind === KIND_AGENT_USER_INPUT_REQUESTED) ||
              left.created_at - right.created_at ||
              left.id.localeCompare(right.id),
          );
          const authorizedHistory = ordered.filter(authorizeEvent);
          const pendingIds = new Set(
            derivePendingUserInputs(authorizedHistory, currentPubkey).map(
              ({ event }) => event.id,
            ),
          );
          setVisibleRequestIds((current) => {
            const next = new Set(current);
            for (const id of pendingIds) next.add(id);
            return next;
          });
          setEvents((current) => {
            const byId = new Map(current.map((event) => [event.id, event]));
            for (const event of authorizedHistory) byId.set(event.id, event);
            return [...byId.values()]
              .sort((left, right) => right.created_at - left.created_at)
              .slice(0, RETAINED_EVENTS);
          });
        }
      } catch (error) {
        console.error("Failed to load agent questions", error);
      }
    };

    void load();
    return () => {
      cancelled = true;
      void dispose?.();
    };
  }, [channelId, currentPubkey, ownedAgentPubkeys]);

  const pending = React.useMemo(
    () =>
      derivePendingUserInputs(events, currentPubkey, optimisticallyResolved),
    [currentPubkey, events, optimisticallyResolved],
  );
  const sent = React.useMemo(() => {
    return derivePendingUserInputs(events, currentPubkey).filter(({ event }) =>
      sentRequestIds.has(event.id),
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
