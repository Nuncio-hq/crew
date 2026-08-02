import * as React from "react";

import {
  mergeWorkingAgentPubkeys,
  useConversationWorkingAgentPubkeys,
} from "@/features/agents/agentWorkingSignal";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import {
  getPendingAgentPubkeysForConversation,
  subscribeMessageEditApplied,
} from "@/features/agents/dispatchedEventIds";
import { getRegisteredObserverAgentPubkeys } from "@/features/agents/observerRelayStore";
import type { ChannelPaneProps } from "@/features/channels/ui/ChannelPane.types";
import { useStableArrayShallow } from "@/shared/hooks/useStableReference";

function agentOnlyPendingPubkeys(pending: readonly string[]): string[] {
  const known = getRegisteredObserverAgentPubkeys();
  if (known.size === 0) return [...pending];
  return pending.filter((pubkey) => known.has(pubkey));
}

type BotTypingEntry = ChannelPaneProps["botTypingEntries"][number];

/** Working bot pubkeys for the open thread composer (typing + observer + queued). */
export function useThreadComposerBotActivity(
  channelId: string | null | undefined,
  openThreadHeadId: string | null | undefined,
  botTypingEntries: readonly BotTypingEntry[],
) {
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

  const pendingPubkeys = React.useMemo(() => {
    void pendingVersion;
    return agentOnlyPendingPubkeys(
      getPendingAgentPubkeysForConversation(threadComposerConversationId),
    );
  }, [pendingVersion, threadComposerConversationId]);

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
