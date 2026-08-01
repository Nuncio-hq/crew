import * as React from "react";
import { useIdentityQuery } from "@/shared/api/hooks";
import { relayClient } from "@/shared/api/relayClient";
import { buildChannelUserInputFilter } from "@/shared/api/relayChannelFilters";
import type { RelayEvent } from "@/shared/api/types";
import { sendChannelUserInputAnswer } from "@/shared/api/tauriUserInput";
import {
  buildSkippedAnswers,
  buildUserInputAnswers,
  derivePendingUserInputs,
  type UserInputAnswers,
  type UserInputEvent,
} from "@/features/channels/lib/userInput";

const RETAINED_EVENTS = 200;

export function useChannelUserInput(channelId: string | null) {
  const identityQuery = useIdentityQuery();
  const currentPubkey = identityQuery.data?.pubkey ?? "";
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

  React.useEffect(() => {
    setEvents([]);
    setOptimisticallyResolved(new Set());
    setSentRequestIds(new Set());
    if (!channelId) return;

    let cancelled = false;
    const filter = buildChannelUserInputFilter(channelId, RETAINED_EVENTS);
    const onEvent = (event: RelayEvent) => {
      if (cancelled) return;
      setEvents((current) => {
        if (current.some((existing) => existing.id === event.id))
          return current;
        return [event, ...current].slice(0, RETAINED_EVENTS);
      });
    };

    const load = async () => {
      try {
        const dispose = await relayClient.subscribeLive(filter, onEvent);
        if (cancelled) {
          await dispose();
          return;
        }
        const history = await relayClient.fetchEvents(filter);
        if (!cancelled) {
          setEvents((current) => {
            const byId = new Map(current.map((event) => [event.id, event]));
            for (const event of history) byId.set(event.id, event);
            return [...byId.values()]
              .sort((left, right) => right.created_at - left.created_at)
              .slice(0, RETAINED_EVENTS);
          });
        }
        return dispose;
      } catch (error) {
        console.error("Failed to load agent questions", error);
      }
    };

    let dispose: (() => Promise<void>) | undefined;
    void load().then((cleanup) => {
      dispose = cleanup;
      if (cancelled) void cleanup?.();
    });
    return () => {
      cancelled = true;
      void dispose?.();
    };
  }, [channelId]);

  const pending = React.useMemo(
    () =>
      derivePendingUserInputs(events, currentPubkey, optimisticallyResolved),
    [currentPubkey, events, optimisticallyResolved],
  );
  const sent = React.useMemo(() => {
    const active = new Set(
      derivePendingUserInputs(events, currentPubkey).map(
        ({ event }) => event.id,
      ),
    );
    return derivePendingUserInputs(events, currentPubkey, new Set())
      .filter(
        ({ event }) => sentRequestIds.has(event.id) && active.has(event.id),
      )
      .map((request) => request);
  }, [currentPubkey, events, sentRequestIds]);

  const answer = React.useCallback(
    async (request: UserInputEvent, answers: UserInputAnswers) => {
      setSendingRequestId(request.event.id);
      try {
        await sendChannelUserInputAnswer(
          channelId ?? request.request.channel_id,
          request.event.id,
          buildUserInputAnswers(answers),
        );
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
    pending,
    sent,
    currentPubkey,
    sendingRequestId,
    sentRequestIds,
    answer,
    skip,
    isLoading: Boolean(channelId) && identityQuery.isLoading,
  };
}
