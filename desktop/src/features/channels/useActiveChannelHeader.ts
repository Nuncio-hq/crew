import * as React from "react";

import {
  useManagedAgentsQuery,
  usePersonasQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { personaAvatarById } from "@/features/agents/lib/agentCardAvatar";
import { mergeAgentNamesIntoProfiles } from "@/features/channels/ui/useChannelActivityTyping";
import { useEphemeralChannelDisplay } from "@/features/channels/useEphemeralChannelDisplay";
import { usePresenceQuery } from "@/features/presence/hooks";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { resolveUserLabel } from "@/features/profile/lib/identity";
import { resolveChannelDisplayLabel } from "@/features/sidebar/lib/channelLabels";
import type { Channel, PresenceStatus } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

export type ActiveDmHeaderParticipant = {
  pubkey: string;
  displayName: string;
  avatarUrl: string | null;
};

export function useActiveChannelHeader(
  activeChannel: Channel | null,
  currentPubkey?: string,
) {
  const activeDmParticipants = React.useMemo(() => {
    if (activeChannel?.channelType !== "dm") {
      return [];
    }

    const normalizedCurrentPubkey = currentPubkey
      ? normalizePubkey(currentPubkey)
      : null;

    return activeChannel.participantPubkeys
      .map((pubkey, index) => ({
        fallbackName: activeChannel.participants[index] ?? null,
        pubkey,
      }))
      .filter(
        (participant) =>
          normalizePubkey(participant.pubkey) !== normalizedCurrentPubkey,
      );
  }, [activeChannel, currentPubkey]);
  const activeDmParticipantPubkeys = React.useMemo(
    () => activeDmParticipants.map((participant) => participant.pubkey),
    [activeDmParticipants],
  );
  const activeDmPresenceQuery = usePresenceQuery(activeDmParticipantPubkeys, {
    enabled: activeDmParticipantPubkeys.length > 0,
  });
  const activeDmProfilesQuery = useUsersBatchQuery(activeDmParticipantPubkeys, {
    enabled: activeDmParticipantPubkeys.length > 0,
  });
  const managedAgentsQuery = useManagedAgentsQuery({
    enabled: activeDmParticipantPubkeys.length > 0,
  });
  const personasQuery = usePersonasQuery({
    enabled: activeDmParticipantPubkeys.length > 0,
  });
  const relayAgentsQuery = useRelayAgentsQuery({
    enabled: activeDmParticipantPubkeys.length > 0,
  });
  const dmProfiles = React.useMemo(
    () =>
      mergeAgentNamesIntoProfiles(
        activeDmProfilesQuery.data?.profiles ?? {},
        managedAgentsQuery.data ?? [],
        relayAgentsQuery.data ?? [],
        currentPubkey,
        personaAvatarById(personasQuery.data ?? []),
      ),
    [
      activeDmProfilesQuery.data?.profiles,
      currentPubkey,
      managedAgentsQuery.data,
      personasQuery.data,
      relayAgentsQuery.data,
    ],
  );
  const activeChannelEphemeralDisplay =
    useEphemeralChannelDisplay(activeChannel);
  const activeDmPresenceStatus: PresenceStatus | null =
    activeDmParticipantPubkeys.length > 0
      ? (activeDmPresenceQuery.data?.[
          activeDmParticipantPubkeys[0]?.toLowerCase()
        ] ?? null)
      : null;
  const activeDmAvatarUrl =
    activeDmParticipantPubkeys.length > 0
      ? (dmProfiles[normalizePubkey(activeDmParticipantPubkeys[0] ?? "")]
          ?.avatarUrl ?? null)
      : null;
  const activeDmHeaderParticipants = React.useMemo(
    () =>
      activeDmParticipants.map((participant) => {
        const profile = dmProfiles[normalizePubkey(participant.pubkey)] ?? null;

        return {
          pubkey: participant.pubkey,
          displayName: resolveUserLabel({
            currentPubkey,
            fallbackName: participant.fallbackName,
            profiles: dmProfiles,
            pubkey: participant.pubkey,
          }),
          avatarUrl: profile?.avatarUrl ?? null,
        };
      }),
    [activeDmParticipants, currentPubkey, dmProfiles],
  );

  return {
    activeChannelTitle: activeChannel
      ? resolveChannelDisplayLabel(activeChannel, currentPubkey, dmProfiles)
      : "Channels",
    activeDmAvatarUrl,
    activeDmHeaderParticipants,
    activeDmPresenceStatus,
    activeChannelEphemeralDisplay,
  };
}
