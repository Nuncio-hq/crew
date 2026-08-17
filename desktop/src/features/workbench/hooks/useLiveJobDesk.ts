import * as React from "react";

import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import { useActiveAgentsForConversation } from "@/features/agents/activeAgentTurnsStore";
import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useChannelUserInput } from "@/features/channels/hooks/useChannelUserInput";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  collectLiveJobSignals,
  shouldShowLiveJobDesk,
} from "../lib/liveJobDesk";
import { userInputBelongsToThread } from "../lib/workbenchTranscript";
import { findWorkbenchRow } from "../lib/workbenchThreadIndex";
import { useWorkbenchThreadIndex } from "./useWorkbenchThreadIndex";

export function useLiveJobDesk(args: {
  channelId: string;
  threadRootId: string;
}) {
  const conversationId = deriveAgentConversationIdOrNull(
    args.channelId,
    args.threadRootId,
  );
  const activeAgentPubkeys = useActiveAgentsForConversation(
    conversationId ?? args.threadRootId,
  );
  const userInput = useChannelUserInput(args.channelId);
  const pendingOnThread = React.useMemo(
    () =>
      userInput.pending.filter((item) =>
        userInputBelongsToThread(item.event, args.threadRootId),
      ),
    [args.threadRootId, userInput.pending],
  );
  const index = useWorkbenchThreadIndex({
    channelId: args.channelId,
    threadRootId: args.threadRootId,
  });
  const missionRow = React.useMemo(
    () => findWorkbenchRow(index.rows, args.channelId, args.threadRootId),
    [args.channelId, args.threadRootId, index.rows],
  );
  const signals = React.useMemo(
    () =>
      collectLiveJobSignals({
        hasActiveTurn: activeAgentPubkeys.length > 0,
        hasPendingUserInput: pendingOnThread.length > 0,
        missionStatus: missionRow?.status ?? null,
      }),
    [activeAgentPubkeys.length, missionRow?.status, pendingOnThread.length],
  );
  const show = shouldShowLiveJobDesk(signals);
  const managedAgents = useManagedAgentsQuery({ enabled: show }).data ?? [];
  const targetPubkey = React.useMemo(() => {
    if (activeAgentPubkeys[0]) return normalizePubkey(activeAgentPubkeys[0]);
    const pendingPubkey = pendingOnThread[0]?.event.pubkey;
    if (pendingPubkey) return normalizePubkey(pendingPubkey);
    return missionRow?.agents[0]?.pubkey
      ? normalizePubkey(missionRow.agents[0].pubkey)
      : null;
  }, [activeAgentPubkeys, missionRow?.agents, pendingOnThread]);
  const targetName = React.useMemo(() => {
    if (!targetPubkey) return "Agent";
    const named =
      managedAgents.find(
        (agent) => normalizePubkey(agent.pubkey) === targetPubkey,
      )?.name ??
      missionRow?.agents.find(
        (agent) => normalizePubkey(agent.pubkey) === targetPubkey,
      )?.name;
    return named || "Agent";
  }, [managedAgents, missionRow?.agents, targetPubkey]);

  return {
    conversationId,
    show,
    targetName,
    targetPubkey,
  };
}
