import * as React from "react";

import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { detectPrefixQuery } from "@/shared/lib/detectPrefixQuery";
import type { Channel } from "@/shared/api/types";
import type { AutocompleteEdit } from "./useRichTextEditor";

export type ChannelSuggestion = {
  id: string;
  name: string;
  channelType: "stream" | "forum";
};

const CHANNEL_QUERY_DEBOUNCE_MS = 120;

/**
 * Archived channels must stay resolvable in historical links (rendered from
 * the unfiltered ChannelNavigationContext list), but are dead ends for new
 * `#channel` references — exclude them here, at generation time, rather than
 * from the shared channel list.
 */
function isChannelSuggestable(
  channel: Pick<Channel, "channelType" | "archivedAt">,
): boolean {
  return channel.channelType !== "dm" && channel.archivedAt === null;
}

/** Exported for unit testing. */
export function selectChannelSuggestions(
  channels: Channel[],
  query: string,
): ChannelSuggestion[] {
  const lowerQuery = query.toLowerCase();
  return channels
    .filter(
      (ch) =>
        isChannelSuggestable(ch) && ch.name.toLowerCase().includes(lowerQuery),
    )
    .slice(0, 8)
    .map((ch) => ({
      id: ch.id,
      name: ch.name,
      channelType: ch.channelType as "stream" | "forum",
    }));
}

export function useChannelLinks() {
  const { channels } = useChannelNavigation();

  const [channelQuery, setChannelQuery] = React.useState<string | null>(null);
  const [channelStartIndex, setChannelStartIndex] = React.useState(0);
  const [channelSelectedIndex, setChannelSelectedIndex] = React.useState(0);

  const debounceTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const currentQueryRef =
    React.useRef<ReturnType<typeof detectPrefixQuery>>(null);
  const latestCursorRef = React.useRef<number>(0);

  /** Channel names (original casing) for overlay highlighting. */
  const knownChannelNames = React.useMemo<string[]>(
    () => channels.filter(isChannelSuggestable).map((ch) => ch.name),
    [channels],
  );

  /** Lower-cased channel names for case-insensitive prefix matching. */
  const knownNamesLower = React.useMemo<string[]>(
    () => knownChannelNames.map((n) => n.toLowerCase()),
    [knownChannelNames],
  );

  const knownNamesLowerRef = React.useRef<string[]>(knownNamesLower);

  // Keep the known-names ref in sync so the debounced callback never reads stale data.
  React.useEffect(() => {
    knownNamesLowerRef.current = knownNamesLower;
  }, [knownNamesLower]);

  React.useEffect(() => {
    return () => {
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  const channelSuggestions = React.useMemo<ChannelSuggestion[]>(() => {
    if (channelQuery === null) {
      return [];
    }
    return selectChannelSuggestions(channels, channelQuery);
  }, [channels, channelQuery]);

  const isChannelOpen = channelQuery !== null && channelSuggestions.length > 0;

  const isCurrentQuery = React.useCallback(() => {
    const current = currentQueryRef.current;
    return (
      current !== null &&
      current.query === channelQuery &&
      current.startIndex === channelStartIndex
    );
  }, [channelQuery, channelStartIndex]);

  const insertChannel = React.useCallback(
    (
      suggestion: ChannelSuggestion,
      selectionEnd: number,
    ): AutocompleteEdit | null => {
      // Pointer callbacks may still belong to the previous rendered menu.
      if (
        !isCurrentQuery() ||
        selectionEnd !== latestCursorRef.current ||
        !channelSuggestions.some((entry) => entry.id === suggestion.id)
      )
        return null;
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }

      const insertText = `#${suggestion.name} `;

      currentQueryRef.current = null;
      setChannelQuery(null);
      setChannelSelectedIndex(0);

      return {
        replaceFromOffset: channelStartIndex,
        replaceToOffset: selectionEnd,
        insertText,
      };
    },
    [channelStartIndex, channelSuggestions, isCurrentQuery],
  );

  const updateChannelQuery = React.useCallback(
    (value: string, cursorPosition: number) => {
      const query = detectPrefixQuery(
        "#",
        value,
        cursorPosition,
        knownNamesLowerRef.current,
      );
      const previous = currentQueryRef.current;
      currentQueryRef.current = query;
      latestCursorRef.current = cursorPosition;
      // Close stale suggestions now, before Enter/Tab or a queued pointer
      // callback can select from them. Only opening the new list is debounced.
      if (
        !query ||
        query.query !== previous?.query ||
        query.startIndex !== previous?.startIndex
      ) {
        setChannelQuery(null);
        setChannelSelectedIndex(0);
      }
      if (debounceTimerRef.current !== null) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }
      if (!query) return;
      debounceTimerRef.current = setTimeout(() => {
        debounceTimerRef.current = null;
        setChannelQuery(query.query);
        setChannelStartIndex(query.startIndex);
        setChannelSelectedIndex(0);
      }, CHANNEL_QUERY_DEBOUNCE_MS);
    },
    [],
  );

  const clearChannels = React.useCallback(() => {
    currentQueryRef.current = null;
    if (debounceTimerRef.current !== null) {
      clearTimeout(debounceTimerRef.current);
      debounceTimerRef.current = null;
    }
    setChannelQuery(null);
    setChannelSelectedIndex(0);
  }, []);

  const handleChannelKeyDown = React.useCallback(
    (
      event: React.KeyboardEvent,
    ): { handled: boolean; suggestion?: ChannelSuggestion } => {
      if (!isChannelOpen || !isCurrentQuery()) {
        return { handled: false };
      }

      if (event.key === "ArrowDown") {
        event.preventDefault();
        setChannelSelectedIndex((current) =>
          current < channelSuggestions.length - 1 ? current + 1 : 0,
        );
        return { handled: true };
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        setChannelSelectedIndex((current) =>
          current > 0 ? current - 1 : channelSuggestions.length - 1,
        );
        return { handled: true };
      }

      // Forward Tab selects; Shift+Tab deliberately does not. The reverse
      // move stays the browser's, so this overlay can't swallow a keyboard
      // user's way back out (see useMentions for the same split).
      if (
        (event.key === "Tab" && !event.shiftKey) ||
        (event.key === "Enter" &&
          !event.ctrlKey &&
          !event.metaKey &&
          !event.altKey &&
          !event.shiftKey)
      ) {
        event.preventDefault();
        return {
          handled: true,
          suggestion: channelSuggestions[channelSelectedIndex],
        };
      }

      if (event.key === "Escape") {
        event.preventDefault();
        clearChannels();
        return { handled: true };
      }

      return { handled: false };
    },
    [
      isChannelOpen,
      isCurrentQuery,
      channelSelectedIndex,
      channelSuggestions,
      clearChannels,
    ],
  );

  return {
    channels,
    channelQuery,
    channelSelectedIndex,
    channelSuggestions,
    clearChannels,
    handleChannelKeyDown,
    insertChannel,
    isChannelOpen,
    knownChannelNames,
    updateChannelQuery,
  };
}

export type UseChannelLinksResult = ReturnType<typeof useChannelLinks>;
