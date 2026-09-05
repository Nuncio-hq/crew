import { ChevronDown, Hash, MessageSquare, Search } from "lucide-react";
import * as React from "react";

import { useChannelsQuery } from "@/features/channels/hooks";
import {
  mergeOpenChannelDirectory,
  useOpenChannelDirectoryQuery,
} from "@/features/channels/openChannelDirectory";
import { ChannelBrowserDialog } from "@/features/channels/ui/ChannelBrowserDialog";
import type { ProjectSelectionItem } from "@/features/projects/lib/projectSelection";
import { projectSelectionChannelCandidates } from "@/features/projects/lib/projectSelection";
import { joinChannel } from "@/shared/api/tauri";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

const ACTION_CLASS =
  "-mx-2 h-7 w-[calc(100%+1rem)] justify-start gap-3 rounded-md px-2 text-left text-sm font-normal hover:bg-muted/70 [&_svg]:h-4 [&_svg]:w-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground";

export function ProjectSelectionDiscussAction({
  items,
  onSelectChannel,
  testIdPrefix = "projects-selection",
}: {
  items: ProjectSelectionItem[];
  onSelectChannel: (channelId: string) => void;
  testIdPrefix?: string;
}) {
  const [expanded, setExpanded] = React.useState(false);
  const [browserOpen, setBrowserOpen] = React.useState(false);
  const channelsQuery = useChannelsQuery();
  const channels = channelsQuery.data ?? [];
  const directoryQuery = useOpenChannelDirectoryQuery({ enabled: browserOpen });
  const browserChannels = React.useMemo(
    () => mergeOpenChannelDirectory(channels, directoryQuery.data),
    [channels, directoryQuery.data],
  );
  const channelsById = new Map(
    channels.map((channel) => [channel.id, channel]),
  );
  const candidates = projectSelectionChannelCandidates(items);

  return (
    <>
      <Button
        aria-expanded={expanded}
        className={ACTION_CLASS}
        data-testid={`${testIdPrefix}-discuss`}
        onClick={() => setExpanded((current) => !current)}
        size="sm"
        type="button"
        variant="ghost"
      >
        <MessageSquare className="h-3.5 w-3.5" />
        Discuss in a channel
        <ChevronDown
          className={cn(
            "ml-auto h-3.5 w-3.5 transition-transform",
            !expanded && "-rotate-90",
          )}
        />
      </Button>
      {expanded ? (
        <div
          className="ml-5 space-y-0.5 border-border/50 border-l pl-2"
          data-testid={`${testIdPrefix}-channel-choices`}
        >
          {candidates.map(({ channelId, count }) => {
            const channel = channelsById.get(channelId);
            const name = channel?.name?.trim() || channelId.slice(0, 8);
            return (
              <Button
                className="h-7 w-full justify-start gap-2 px-2 text-left text-xs font-normal"
                data-testid={`${testIdPrefix}-related-channel`}
                disabled={!channel}
                title={
                  !channel
                    ? channelsQuery.isPending
                      ? "Channels are loading"
                      : "Channel unavailable"
                    : undefined
                }
                key={channelId}
                onClick={() => onSelectChannel(channelId)}
                size="sm"
                type="button"
                variant="ghost"
              >
                <Hash className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate">#{name}</span>
                {count > 1 ? (
                  <span className="shrink-0 tabular-nums text-muted-foreground">
                    {count}
                  </span>
                ) : null}
              </Button>
            );
          })}
          <Button
            className="h-7 w-full justify-start gap-2 px-2 text-left text-xs font-normal"
            data-testid={`${testIdPrefix}-search-channels`}
            onClick={() => setBrowserOpen(true)}
            size="sm"
            type="button"
            variant="ghost"
          >
            <Search className="h-3.5 w-3.5 text-muted-foreground" />
            Search channels…
          </Button>
        </div>
      ) : null}
      <ChannelBrowserDialog
        channels={browserChannels}
        channelTypeFilter="stream"
        onJoinChannel={async (channelId) => {
          await joinChannel(channelId);
          await channelsQuery.refetch({ throwOnError: true });
        }}
        onOpenChange={setBrowserOpen}
        onSelectChannel={onSelectChannel}
        open={browserOpen}
      />
    </>
  );
}
