import type { BotActivityAgent } from "@/features/channels/ui/BotActivityBar";
import type { ChannelAgentSessionAgent } from "@/features/channels/ui/useChannelAgentSessions";
import type { Channel } from "@/shared/api/types";

/**
 * When the panel was opened from a different channel than the currently active
 * one, re-scope it to the active channel so that both the content/header AND
 * channel-backed actions (e.g. Stop current turn) operate on the same channel
 * object.
 */
export function resolveAgentSessionChannelId({
  activeChannelId,
  openAgentSessionChannelId,
}: {
  activeChannelId: string | null;
  openAgentSessionChannelId: string | null;
}): string | null {
  if (
    openAgentSessionChannelId &&
    activeChannelId &&
    activeChannelId !== openAgentSessionChannelId
  ) {
    return activeChannelId;
  }
  return openAgentSessionChannelId;
}

export function resolveAgentSessionChannelBinding({
  activeChannel,
  activeChannelId,
  activityAgents,
  isAgentInActivityList,
  openAgentSessionChannelId,
  selectedAgent,
}: {
  activeChannel: Channel;
  activeChannelId: string | null;
  activityAgents: BotActivityAgent[];
  isAgentInActivityList: (input: {
    activityAgents: BotActivityAgent[];
    selectedAgent: ChannelAgentSessionAgent | null;
  }) => boolean;
  openAgentSessionChannelId: string | null;
  selectedAgent: ChannelAgentSessionAgent;
}): { channel: Channel | null; channelId: string | null } {
  const channelId = resolveAgentSessionChannelId({
    activeChannelId,
    openAgentSessionChannelId,
  });
  if (channelId) {
    return {
      channel: channelId === activeChannel.id ? activeChannel : null,
      channelId,
    };
  }
  return {
    channel: isAgentInActivityList({ activityAgents, selectedAgent })
      ? activeChannel
      : null,
    channelId,
  };
}
