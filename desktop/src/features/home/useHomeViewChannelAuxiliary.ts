import * as React from "react";

import type { Channel } from "@/shared/api/types";

export function useHomeViewChannelAuxiliary(
  channels: Channel[] | undefined,
) {
  const [managedChannelId, setManagedChannelId] = React.useState<string | null>(
    null,
  );
  const [membersChannel, setMembersChannel] = React.useState<Channel | null>(
    null,
  );

  const managedChannel = React.useMemo(() => {
    if (!managedChannelId || !channels) return null;
    return channels.find((channel) => channel.id === managedChannelId) ?? null;
  }, [channels, managedChannelId]);

  const isChannelManagementOpen = managedChannel !== null;

  const handleOpenMembers = React.useCallback(() => {
    setMembersChannel(managedChannel);
  }, [managedChannel]);

  const handleCloseMembers = React.useCallback(() => {
    setMembersChannel(null);
  }, []);

  const handleCloseChannelManagement = React.useCallback(() => {
    setManagedChannelId(null);
  }, []);

  const handleManageChannel = React.useCallback(
    (channelId: string, missionChannelId?: string | null) => {
      setManagedChannelId(missionChannelId ?? channelId);
    },
    [],
  );

  return {
    handleCloseChannelManagement,
    handleCloseMembers,
    handleManageChannel,
    handleOpenMembers,
    isChannelManagementOpen,
    managedChannel,
    membersChannel,
    setManagedChannelId,
  };
}
