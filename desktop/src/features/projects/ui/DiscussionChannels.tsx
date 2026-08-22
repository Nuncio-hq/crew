import { Hash } from "lucide-react";
import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useChannelsQuery } from "@/features/channels/hooks";
import {
  discussionSnippet,
  groupDiscussionChannels,
} from "@/features/projects/lib/discussionChannels";
import { relativeTime } from "@/features/projects/lib/projectsViewHelpers";
import { useSearchMessagesQuery } from "@/features/search/hooks";
import type { SearchHit } from "@/shared/api/searchTypes";
import { cn } from "@/shared/lib/cn";

const DISCUSSION_SEARCH_LIMIT = 500;

function useDiscussionChannels(query: string) {
  const search = useSearchMessagesQuery(query, {
    limit: DISCUSSION_SEARCH_LIMIT,
  });
  const hits = React.useMemo(
    () =>
      [...(search.data?.hits ?? [])].sort((left, right) => {
        return right.createdAt - left.createdAt;
      }),
    [search.data],
  );
  const channels = React.useMemo(() => groupDiscussionChannels(hits), [hits]);
  return {
    channels,
    hits,
    isLoading: search.isLoading,
    isTruncated: hits.length >= DISCUSSION_SEARCH_LIMIT,
  };
}

function useChannelNameLookup(enabled: boolean) {
  const channelsQuery = useChannelsQuery({ enabled });
  return React.useCallback(
    (id: string, nameFromHit: string | null) =>
      nameFromHit ??
      channelsQuery.data?.find((channel) => channel.id === id)?.name ??
      id.slice(0, 8),
    [channelsQuery.data],
  );
}

export function DiscussedInChannels({
  className,
  entityLabel = "this",
  query,
}: {
  className?: string;
  entityLabel?: string;
  query: string;
}) {
  const { channels, hits, isTruncated } = useDiscussionChannels(query);
  const { goChannel, openSearchHit } = useAppNavigation();
  const channelName = useChannelNameLookup(channels.length > 0);
  const latestHitByChannel = React.useMemo(() => {
    const byChannel = new Map<string, SearchHit>();
    for (const hit of hits) {
      if (hit.channelId && !byChannel.has(hit.channelId)) {
        byChannel.set(hit.channelId, hit);
      }
    }
    return byChannel;
  }, [hits]);
  if (channels.length === 0) return null;

  return (
    <div
      className={cn(
        "min-w-0 overflow-hidden rounded-lg border border-border/60 bg-muted/20",
        className,
      )}
    >
      <h4 className="border-b border-border/40 px-3 py-1.5 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
        Channels
      </h4>
      <div className="divide-y divide-border/40">
        {channels.slice(0, 3).map((channel) => {
          const latestHit = latestHitByChannel.get(channel.id);
          if (!latestHit) return null;
          const name = channelName(channel.id, channel.name);
          return (
            <div
              className="flex w-full min-w-0 items-center gap-2 px-3 py-2"
              key={channel.id}
            >
              <Hash className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate text-sm">
                <span className="text-muted-foreground">
                  {channel.participants.length}{" "}
                  {channel.participants.length === 1 ? "person" : "people"}{" "}
                  discussed {entityLabel} in{" "}
                </span>
                <button
                  className="font-medium text-foreground hover:underline"
                  onClick={() => void goChannel(channel.id)}
                  type="button"
                >
                  #{name}
                </button>
                <button
                  className="text-muted-foreground hover:underline"
                  onClick={() => void openSearchHit(latestHit)}
                  type="button"
                >
                  {" "}
                  · {relativeTime(channel.lastActivityAt)} —{" "}
                  {discussionSnippet(latestHit.content)}
                </button>
              </span>
            </div>
          );
        })}
      </div>
      {channels.length > 3 || isTruncated ? (
        <p className="border-t border-border/40 px-3 py-1.5 text-xs text-muted-foreground">
          {channels.length > 3 ? `${channels.length - 3} more channels` : null}
          {channels.length > 3 && isTruncated ? " · " : null}
          {isTruncated ? "Showing the 500 most recent mentions" : null}
        </p>
      ) : null}
    </div>
  );
}

export function DiscussionChannelsPanel({ query }: { query: string }) {
  const { channels, isLoading, isTruncated } = useDiscussionChannels(query);
  const { goChannel } = useAppNavigation();
  const channelName = useChannelNameLookup(channels.length > 0);

  if (isLoading) {
    return (
      <p className="px-4 py-6 text-sm text-muted-foreground">
        Searching channel discussions…
      </p>
    );
  }
  if (channels.length === 0) {
    return (
      <p className="px-4 py-6 text-sm text-muted-foreground">
        No channels reference this repository yet.
      </p>
    );
  }

  return (
    <div>
      <ul className="divide-y divide-border/50">
        {channels.map((channel) => {
          const name = channelName(channel.id, channel.name);
          return (
            <li key={channel.id}>
              <button
                className="flex w-full min-w-0 items-center gap-2.5 px-4 py-3 text-left transition-colors hover:bg-muted/30"
                onClick={() => void goChannel(channel.id)}
                type="button"
              >
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted/50">
                  <Hash className="h-4 w-4 text-muted-foreground" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">
                    #{name}
                  </span>
                  <span className="block truncate text-xs text-muted-foreground">
                    {channel.participants.length} participants ·{" "}
                    {channel.messageCount}
                    {isTruncated ? "+" : ""} messages ·{" "}
                    {relativeTime(channel.lastActivityAt)}
                  </span>
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
