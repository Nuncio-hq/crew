import * as React from "react";

import {
  mergeWorkingAgentPubkeys,
  useConversationWorkingAgentPubkeys,
} from "@/features/agents/agentWorkingSignal";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import {
  filterPendingToKnownAgents,
  getPendingAgentPubkeysForConversation,
  subscribeMessageEditApplied,
} from "@/features/agents/dispatchedEventIds";
import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import type { ChannelPaneProps } from "@/features/channels/ui/ChannelPane.types";
import { useStableArrayShallow } from "@/shared/hooks/useStableReference";

type BotTypingEntry = ChannelPaneProps["botTypingEntries"][number];

/** Working bot pubkeys for the open thread composer (typing + observer + queued). */
export function useThreadComposerBotActivity(
  channelId: string | null | undefined,
  openThreadHeadId: string | null | undefined,
  botTypingEntries: readonly BotTypingEntry[],
) {
  const knownAgentPubkeys = useKnownAgentPubkeys();

  const threadComposerBotTypingPubkeys = React.useMemo(() => {
    if (!openThreadHeadId) return [];
    return botTypingEntries
      .filter((entry) => entry.threadHeadId === openThreadHeadId)
      .map((entry) => entry.pubkey)
      .filter(
        (pubkey, index, all) =>
          all.findIndex(
            (candidate) => candidate.toLowerCase() === pubkey.toLowerCase(),
          ) === index,
      );
  }, [botTypingEntries, openThreadHeadId]);

  const threadComposerConversationId = React.useMemo(
    () => deriveAgentConversationIdOrNull(channelId, openThreadHeadId),
    [channelId, openThreadHeadId],
  );

  const threadComposerWorkingBotPubkeys = useConversationWorkingAgentPubkeys(
    threadComposerConversationId,
    threadComposerBotTypingPubkeys,
  );

  const [pendingVersion, setPendingVersion] = React.useState(0);
  React.useEffect(() => {
    return subscribeMessageEditApplied(() => {
      setPendingVersion((current) => current + 1);
    });
  }, []);

  // Identity filter (community known agents), not observer liveness — humans
  // never surface as stoppable even when the observer registry is populated.
  const pendingPubkeys = React.useMemo(() => {
    void pendingVersion;
    return filterPendingToKnownAgents(
      getPendingAgentPubkeysForConversation(threadComposerConversationId),
      knownAgentPubkeys,
    );
  }, [knownAgentPubkeys, pendingVersion, threadComposerConversationId]);

  const mergedWorkingBotPubkeys = useStableArrayShallow(
    React.useMemo(
      () =>
        mergeWorkingAgentPubkeys(
          threadComposerWorkingBotPubkeys,
          pendingPubkeys,
        ),
      [pendingPubkeys, threadComposerWorkingBotPubkeys],
    ),
  );

  return {
    hasThreadComposerBotActivity: mergedWorkingBotPubkeys.length > 0,
    threadComposerConversationId,
    threadComposerWorkingBotPubkeys: mergedWorkingBotPubkeys,
  };
}
