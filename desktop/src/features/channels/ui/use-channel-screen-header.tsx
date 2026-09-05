import * as React from "react";

import type { EphemeralChannelDisplay } from "@/features/channels/lib/ephemeralChannel";
import type { ActiveDmHeaderParticipant } from "@/features/channels/useActiveChannelHeader";
import type { TimelineMessage } from "@/features/messages/types";
import type { Channel, PresenceStatus } from "@/shared/api/types";
import { ChannelScreenHeader } from "./ChannelScreenHeader";

export function useChannelScreenHeader({
  activeChannel,
  activeChannelEphemeralDisplay,
  activeChannelTitle,
  activeDmAvatarUrl,
  activeDmHeaderParticipants,
  activeDmPresenceStatus,
  channelHeaderChromeRef,
  currentPubkey,
  headerEndActions,
  handleManageChannel,
  handleOpenThreadAndCloseAgentSession,
  handleToggleMembers,
  isAddBotOpen,
  isHuddleTranscript,
  isSinglePanelView,
  joinChannelMutation,
  setIsAddBotOpen,
  shouldCompactHeaderActions,
  timelineMessages,
}: {
  activeChannel: Channel | null;
  activeChannelEphemeralDisplay: EphemeralChannelDisplay | null;
  activeChannelTitle: string;
  activeDmAvatarUrl: string | null;
  activeDmHeaderParticipants: ActiveDmHeaderParticipant[];
  activeDmPresenceStatus: PresenceStatus | null;
  channelHeaderChromeRef: React.Ref<HTMLDivElement>;
  currentPubkey: string | undefined;
  headerEndActions?: React.ReactNode;
  handleManageChannel: () => void;
  handleOpenThreadAndCloseAgentSession: (message: TimelineMessage) => void;
  handleToggleMembers: () => void;
  isAddBotOpen: boolean;
  isHuddleTranscript: boolean;
  isSinglePanelView: boolean;
  joinChannelMutation: {
    isPending: boolean;
    mutateAsync: () => Promise<unknown>;
  };
  setIsAddBotOpen: React.Dispatch<React.SetStateAction<boolean>>;
  shouldCompactHeaderActions: boolean;
  timelineMessages: TimelineMessage[];
}) {
  return React.useMemo(
    () => (
      <ChannelScreenHeader
        activeChannel={activeChannel}
        activeChannelEphemeralDisplay={activeChannelEphemeralDisplay}
        activeChannelTitle={activeChannelTitle}
        actionsVariant={shouldCompactHeaderActions ? "compact" : "inline"}
        activeDmAvatarUrl={activeDmAvatarUrl}
        activeDmHeaderParticipants={activeDmHeaderParticipants}
        activeDmPresenceStatus={activeDmPresenceStatus}
        chromeWrapperRef={channelHeaderChromeRef}
        currentPubkey={currentPubkey}
        headerEndActions={headerEndActions}
        isAddBotOpen={isAddBotOpen}
        isJoining={joinChannelMutation.isPending}
        onAddBotOpenChange={setIsAddBotOpen}
        onJoinChannel={(): Promise<void> =>
          joinChannelMutation.mutateAsync().then(() => undefined)
        }
        onManageChannel={handleManageChannel}
        onOpenThread={handleOpenThreadAndCloseAgentSession}
        onToggleMembers={handleToggleMembers}
        showHeaderContent={!isSinglePanelView && !isHuddleTranscript}
        timelineMessages={timelineMessages}
        transparentChrome={activeChannel?.channelType !== "forum"}
      />
    ),
    [
      activeChannel,
      activeChannelEphemeralDisplay,
      activeChannelTitle,
      shouldCompactHeaderActions,
      activeDmAvatarUrl,
      activeDmHeaderParticipants,
      activeDmPresenceStatus,
      channelHeaderChromeRef,
      currentPubkey,
      headerEndActions,
      isAddBotOpen,
      joinChannelMutation.isPending,
      joinChannelMutation.mutateAsync,
      handleManageChannel,
      handleOpenThreadAndCloseAgentSession,
      handleToggleMembers,
      isSinglePanelView,
      isHuddleTranscript,
      timelineMessages,
      setIsAddBotOpen,
    ],
  );
}
