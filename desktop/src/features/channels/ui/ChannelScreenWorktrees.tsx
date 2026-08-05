import * as React from "react";

import { deriveProjectChannelWorkspace } from "@/features/channels/lib/projectChannelWorkspace";
import { ChannelWorktreesDrawer } from "@/features/channels/ui/ChannelWorktreesDrawer";
import type { TimelineMessage } from "@/features/messages/types";

type ChannelScreenWorktreesProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  channelId: string | null;
  timelineMessages: readonly TimelineMessage[];
  onOpenThread: (message: TimelineMessage) => void;
};

export function ChannelScreenWorktrees({
  open,
  onOpenChange,
  channelId,
  timelineMessages,
  onOpenThread,
}: ChannelScreenWorktreesProps) {
  const workspace = React.useMemo(
    () => deriveProjectChannelWorkspace(timelineMessages),
    [timelineMessages],
  );
  return (
    <ChannelWorktreesDrawer
      channelId={channelId}
      channelRootIds={workspace.channelRootIds}
      onOpenChange={onOpenChange}
      onOpenThread={(rootEventId) => {
        const needle = rootEventId.toLowerCase();
        const message = timelineMessages.find(
          (entry) => entry.id.toLowerCase() === needle,
        );
        if (message) onOpenThread(message);
      }}
      open={open}
      repositoryPath={workspace.repositoryPath}
      rootBodiesById={workspace.rootBodiesById}
    />
  );
}

export function useProjectChannelRepositoryPath(
  timelineMessages: readonly TimelineMessage[],
): string | null {
  return React.useMemo(
    () => deriveProjectChannelWorkspace(timelineMessages).repositoryPath,
    [timelineMessages],
  );
}
