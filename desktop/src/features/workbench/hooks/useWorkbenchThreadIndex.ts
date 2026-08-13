import * as React from "react";

import { useAppShell } from "@/app/AppShellContext";
import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useChannelsQuery } from "@/features/channels/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useHomeFeedQuery } from "@/features/home/hooks";
import { buildInboxItems } from "@/features/home/lib/inbox";
import { filterInboxItems } from "@/features/home/lib/inboxViewHelpers";
import { useMissionInboxSections } from "@/features/home/useMissionInboxSections";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import { useCommunities } from "@/features/communities/useCommunities";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import {
  findManagedAgentRuntime,
  isManagedAgentRuntimeSleeping,
} from "@/features/agents/managedAgentRuntimeStatus";
import { useMissionInboxActiveTurns } from "@/features/home/lib/missionInbox";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  deriveWorkbenchThreadIndex,
  ensureSelectedWorkbenchRow,
  groupWorkbenchByAgent,
  groupWorkbenchByChannel,
} from "../lib/workbenchThreadIndex";

export function useWorkbenchThreadIndex(selected?: {
  channelId?: string;
  threadRootId?: string;
}) {
  const channelsQuery = useChannelsQuery();
  const identityQuery = useIdentityQuery();
  const currentPubkey = identityQuery.data?.pubkey;
  const channels = channelsQuery.data ?? [];
  const ownedAgentPubkeys = useCurrentOwnedAgentPubkeys(currentPubkey);
  const homeFeedQuery = useHomeFeedQuery();
  const { getChannelReadAt, getMessageReadAt, getThreadReadAt } = useAppShell();
  const inboxItems = React.useMemo(
    () =>
      filterInboxItems(
        buildInboxItems({
          channels,
          currentPubkey,
          feed: homeFeedQuery.data,
          getChannelReadAt,
          getMessageReadAt,
          getThreadReadAt,
        }),
      ),
    [
      channels,
      currentPubkey,
      getChannelReadAt,
      getMessageReadAt,
      getThreadReadAt,
      homeFeedQuery.data,
    ],
  );
  const missionSections = useMissionInboxSections({
    channels,
    currentPubkey,
    effectiveDoneSet: new Set(),
    inboxItems,
    ownedAgentPubkeys,
  });
  const missionRows = React.useMemo(
    () => [
      ...missionSections.needsYou,
      ...missionSections.readyToReview,
      ...missionSections.working,
    ],
    [missionSections],
  );
  const activeTurns = useMissionInboxActiveTurns();
  const { activeCommunity } = useCommunities();
  const activeAgentPubkeys = React.useMemo(
    () => [...new Set(activeTurns.flatMap((turn) => turn.agentPubkeys))],
    [activeTurns],
  );
  const managedAgentRuntimesQuery = useManagedAgentRuntimesQuery({
    enabled: Boolean(
      activeCommunity?.relayUrl && activeAgentPubkeys.length > 0,
    ),
  });
  const sleepingAgentPubkeys = React.useMemo(() => {
    if (!activeCommunity?.relayUrl) return new Set<string>();
    const sleeping = new Set<string>();
    for (const pubkey of activeAgentPubkeys) {
      const runtime = findManagedAgentRuntime(
        managedAgentRuntimesQuery.data ?? [],
        pubkey,
        activeCommunity.relayUrl,
      );
      if (isManagedAgentRuntimeSleeping(runtime)) {
        sleeping.add(normalizePubkey(pubkey));
      }
    }
    return sleeping;
  }, [
    activeAgentPubkeys,
    activeCommunity?.relayUrl,
    managedAgentRuntimesQuery.data,
  ]);
  const managedAgents = useManagedAgentsQuery({
    enabled: Boolean(currentPubkey),
  }).data;
  const agentNamesByPubkey = React.useMemo(() => {
    const map = new Map<string, string>();
    for (const agent of managedAgents ?? []) {
      map.set(normalizePubkey(agent.pubkey), agent.name);
    }
    return map;
  }, [managedAgents]);
  const knownAgentPubkeys = useKnownAgentPubkeys();
  const unreadRootIds = React.useMemo(() => {
    const unread = new Set<string>();
    for (const item of inboxItems) {
      if (item.unreadCount > 0) unread.add(item.id);
    }
    return unread;
  }, [inboxItems]);

  const rows = React.useMemo(() => {
    const derived = deriveWorkbenchThreadIndex({
      agentNamesByPubkey,
      channels,
      inboxItems,
      missionRows,
      sleepingAgentPubkeys,
      unreadRootIds,
    });
    const channelId = selected?.channelId;
    const threadRootId = selected?.threadRootId;
    if (!channelId || !threadRootId) return derived;
    const channel = channels.find((candidate) => candidate.id === channelId);
    return ensureSelectedWorkbenchRow(derived, {
      channelId,
      channelName: channel?.name ?? channelId,
      threadRootId,
    });
  }, [
    agentNamesByPubkey,
    channels,
    inboxItems,
    missionRows,
    selected?.channelId,
    selected?.threadRootId,
    sleepingAgentPubkeys,
    unreadRootIds,
  ]);

  const byChannel = React.useMemo(() => groupWorkbenchByChannel(rows), [rows]);
  const byAgent = React.useMemo(() => groupWorkbenchByAgent(rows), [rows]);

  return {
    byAgent,
    byChannel,
    knownAgentPubkeys,
    managedAgents: managedAgents ?? [],
    rows,
    sleepingAgentPubkeys,
  };
}
