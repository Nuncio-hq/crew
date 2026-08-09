import * as React from "react";

import { isInboxThreadContextEvent } from "@/features/home/lib/inboxViewHelpers";
import { fetchStructuralAuxForMessages } from "@/features/messages/lib/auxBackfill";
import { getThreadReference } from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import { buildChannelReactionAuxFilter } from "@/shared/api/relayChannelFilters";
import { isVerifiedRelayEvent } from "@/shared/api/relayEventVerification";
import { getEventById } from "@/shared/api/tauri";
import type { FeedItem, RelayEvent } from "@/shared/api/types";
import {
  CHANNEL_TIMELINE_CONTENT_KINDS,
  HOME_MENTION_EVENT_KINDS,
} from "@/shared/constants/kinds";

type InboxThreadContextResult = {
  events: RelayEvent[];
  hasLoadError: boolean;
  isLoading: boolean;
  /** Edits/deletions referencing context messages, fetched by `#e`. */
  structuralEvents: RelayEvent[];
  /** Re-fetch structural events after an Inbox edit is published. */
  refreshStructuralEvents: () => Promise<void>;
  /** kind:7 events referencing the context messages, fetched by `#e`. */
  reactionEvents: RelayEvent[];
  /** Re-fetch reaction events (e.g. after a toggle) without reloading context. */
  refreshReactions: () => Promise<void>;
};

const THREAD_CONTEXT_LIMIT = 100;
const MAX_ANCESTOR_HOPS = 50;
const CHANNEL_CONTEXT_EVENT_KINDS = new Set<number>(
  CHANNEL_TIMELINE_CONTENT_KINDS,
);

function dedupeEvents(events: RelayEvent[]): RelayEvent[] {
  const eventsById = new Map<string, RelayEvent>();
  for (const event of events) {
    eventsById.set(event.id, event);
  }
  return [...eventsById.values()].sort((a, b) => a.created_at - b.created_at);
}

export function deriveInboxThreadSelectionFromVerifiedEvent(
  event: RelayEvent,
  expectedEventId: string,
) {
  if (event.id !== expectedEventId) return null;
  const thread = getThreadReference(event.tags);
  const selectedChannelId = event.tags.find(
    (tag) => tag[0] === "h" && tag[1],
  )?.[1];
  if (!selectedChannelId) return null;
  return {
    selectedChannelId,
    selectedEventId: event.id,
    selectedParentId: thread.parentId,
    selectedThreadRootId: thread.rootId ?? thread.parentId ?? event.id,
  };
}

export function useInboxThreadContext(
  item: FeedItem | null,
  channelMessages: RelayEvent[] | undefined,
  options: {
    fullChannel?: boolean;
    hasChannelLoadError?: boolean;
    isChannelLoading?: boolean;
  } = {},
): InboxThreadContextResult {
  const [fetchedEvents, setFetchedEvents] = React.useState<RelayEvent[]>([]);
  const [hasLoadError, setHasLoadError] = React.useState(false);
  const [isLoading, setIsLoading] = React.useState(false);

  const selectedEventId = item?.id ?? null;
  const fullChannel = options.fullChannel === true;
  const selectedVerifiedEvent = React.useMemo(
    () =>
      [...(channelMessages ?? []), ...fetchedEvents].find(
        (event) => event.id === selectedEventId && isVerifiedRelayEvent(event),
      ) ?? null,
    [channelMessages, fetchedEvents, selectedEventId],
  );
  const selectedVerifiedSelection = React.useMemo(
    () =>
      selectedVerifiedEvent && selectedEventId
        ? deriveInboxThreadSelectionFromVerifiedEvent(
            selectedVerifiedEvent,
            selectedEventId,
          )
        : null,
    [selectedEventId, selectedVerifiedEvent],
  );
  const selectedChannelId =
    selectedVerifiedSelection?.selectedChannelId ?? null;

  React.useEffect(() => {
    let isCancelled = false;

    if (fullChannel || !selectedEventId) {
      setFetchedEvents([]);
      setHasLoadError(false);
      setIsLoading(false);
      return () => {
        isCancelled = true;
      };
    }

    async function loadContext() {
      const targetEventId = selectedEventId;
      if (!targetEventId) {
        return;
      }

      setIsLoading(true);
      setHasLoadError(false);

      try {
        const targetEvent = await getEventById(targetEventId);
        if (!isVerifiedRelayEvent(targetEvent)) {
          throw new Error("selected Inbox event failed signature verification");
        }
        const selection = deriveInboxThreadSelectionFromVerifiedEvent(
          targetEvent,
          targetEventId,
        );
        if (!selection) {
          throw new Error(
            "selected Inbox event identity or signed channel authority mismatch",
          );
        }
        const eventsById = new Map<string, RelayEvent>([
          [targetEvent.id, targetEvent],
        ]);
        let ancestorFailed = false;
        const fetchEvent = async (eventId: string) => {
          if (eventsById.has(eventId)) return eventsById.get(eventId) ?? null;
          try {
            const event = await getEventById(eventId);
            if (!isVerifiedRelayEvent(event) || event.id !== eventId) {
              ancestorFailed = true;
              return null;
            }
            eventsById.set(event.id, event);
            return event;
          } catch {
            ancestorFailed = true;
            return null;
          }
        };
        const ancestorEventsPromise = (async () => {
          if (selection.selectedThreadRootId !== targetEventId) {
            await fetchEvent(selection.selectedThreadRootId);
          }

          let ancestorId = selection.selectedParentId;
          const seen = new Set<string>([targetEventId]);
          let hops = 0;
          while (
            ancestorId &&
            !seen.has(ancestorId) &&
            hops < MAX_ANCESTOR_HOPS
          ) {
            seen.add(ancestorId);
            const ancestor = await fetchEvent(ancestorId);
            if (!ancestor || ancestorId === selection.selectedThreadRootId) {
              break;
            }
            ancestorId = getThreadReference(ancestor.tags).parentId;
            hops += 1;
          }
          return {
            events: [...eventsById.values()],
            failed: ancestorFailed,
          };
        })();

        const descendantEventsPromise = selection.selectedChannelId
          ? relayClient
              .fetchEvents({
                "#e": [selection.selectedThreadRootId],
                "#h": [selection.selectedChannelId],
                kinds: [...HOME_MENTION_EVENT_KINDS],
                limit: THREAD_CONTEXT_LIMIT,
              })
              .then((events) => ({ events, failed: false }))
              .catch((error) => {
                console.error(
                  "Failed to hydrate Inbox thread context",
                  selection.selectedChannelId,
                  selection.selectedThreadRootId,
                  error,
                );
                return { events: [] as RelayEvent[], failed: true };
              })
          : Promise.resolve({ events: [] as RelayEvent[], failed: false });
        const [ancestorResult, descendantResult] = await Promise.all([
          ancestorEventsPromise,
          descendantEventsPromise,
        ]);

        if (isCancelled) {
          return;
        }

        setHasLoadError(ancestorResult.failed || descendantResult.failed);
        setFetchedEvents(
          dedupeEvents(
            [...ancestorResult.events, ...descendantResult.events].filter(
              (event): event is RelayEvent =>
                event !== null &&
                isVerifiedRelayEvent(event) &&
                isInboxThreadContextEvent(event, selection),
            ),
          ),
        );
      } catch (error) {
        if (!isCancelled) {
          console.error("Failed to load Inbox message context", error);
          setHasLoadError(true);
        }
      } finally {
        if (!isCancelled) {
          setIsLoading(false);
        }
      }
    }

    void loadContext();

    return () => {
      isCancelled = true;
    };
  }, [selectedEventId, fullChannel]);

  const events = React.useMemo(() => {
    if (!selectedVerifiedEvent || !selectedVerifiedSelection) {
      return [];
    }

    if (fullChannel) {
      return dedupeEvents([
        selectedVerifiedEvent,
        ...(channelMessages ?? []).filter(
          (event) =>
            CHANNEL_CONTEXT_EVENT_KINDS.has(event.kind) &&
            isVerifiedRelayEvent(event),
        ),
      ]);
    }

    const localContext = (channelMessages ?? []).filter((event) => {
      return (
        isVerifiedRelayEvent(event) &&
        isInboxThreadContextEvent(event, selectedVerifiedSelection)
      );
    });

    const currentFetchedEvents = fetchedEvents.filter((event) =>
      isInboxThreadContextEvent(event, selectedVerifiedSelection),
    );

    return dedupeEvents([
      selectedVerifiedEvent,
      ...currentFetchedEvents,
      ...localContext,
    ]);
  }, [
    channelMessages,
    fetchedEvents,
    fullChannel,
    selectedVerifiedEvent,
    selectedVerifiedSelection,
  ]);

  // Auxiliary events carry only an `#e` reference, so they may be absent from
  // both the selected feed item and the channel-window cache. Hydrate them by
  // the context message ids so cold Inbox items receive edits, deletions, and
  // reactions without requiring the full channel timeline to be open.
  const contextEventIdsKey = React.useMemo(
    () =>
      events
        .map((event) => event.id)
        .sort()
        .join(","),
    [events],
  );
  const [structuralEvents, setStructuralEvents] = React.useState<RelayEvent[]>(
    [],
  );

  const fetchStructuralEvents = React.useCallback(async (): Promise<
    RelayEvent[] | null
  > => {
    const eventIds = contextEventIdsKey ? contextEventIdsKey.split(",") : [];
    if (!selectedChannelId || eventIds.length === 0) {
      return [];
    }

    try {
      // Two hops, not one. A deletion can target an edit event rather than the
      // original message, and `formatTimelineMessages` drops an edit only when
      // the edit's own id is in the deletion set. A one-hop fetch therefore
      // re-applies retracted content on a cold Inbox open. The channel and
      // thread paths already resolve this closure with the same helper.
      return await fetchStructuralAuxForMessages(selectedChannelId, eventIds);
    } catch (error) {
      console.error(
        "Failed to hydrate structural events for Inbox context messages",
        selectedChannelId,
        error,
      );
      return null;
    }
  }, [contextEventIdsKey, selectedChannelId]);

  React.useEffect(() => {
    let isCancelled = false;
    setStructuralEvents([]);

    void fetchStructuralEvents().then((fetched) => {
      if (!isCancelled && fetched !== null) {
        setStructuralEvents(fetched);
      }
    });

    return () => {
      isCancelled = true;
    };
  }, [fetchStructuralEvents]);

  const refreshStructuralEvents = React.useCallback(async () => {
    const fetched = await fetchStructuralEvents();
    if (fetched !== null) {
      setStructuralEvents(fetched);
    }
  }, [fetchStructuralEvents]);

  const [reactionEvents, setReactionEvents] = React.useState<RelayEvent[]>([]);

  const fetchReactions = React.useCallback(async (): Promise<
    RelayEvent[] | null
  > => {
    const eventIds = contextEventIdsKey ? contextEventIdsKey.split(",") : [];
    if (!selectedChannelId || eventIds.length === 0) {
      return [];
    }

    try {
      return await relayClient.fetchAuxEventsByReference(
        selectedChannelId,
        eventIds,
        buildChannelReactionAuxFilter,
      );
    } catch (error) {
      console.error(
        "Failed to hydrate reactions for Inbox context messages",
        selectedChannelId,
        error,
      );
      return null;
    }
  }, [contextEventIdsKey, selectedChannelId]);

  React.useEffect(() => {
    let isCancelled = false;
    setReactionEvents([]);

    void fetchReactions().then((fetched) => {
      if (!isCancelled && fetched !== null) {
        setReactionEvents(fetched);
      }
    });

    return () => {
      isCancelled = true;
    };
  }, [fetchReactions]);

  const refreshReactions = React.useCallback(async () => {
    const fetched = await fetchReactions();
    if (fetched !== null) {
      setReactionEvents(fetched);
    }
  }, [fetchReactions]);

  return {
    events,
    hasLoadError: fullChannel
      ? options.hasChannelLoadError === true
      : hasLoadError,
    isLoading: fullChannel ? options.isChannelLoading === true : isLoading,
    structuralEvents,
    refreshStructuralEvents,
    reactionEvents,
    refreshReactions,
  };
}
