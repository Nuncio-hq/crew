import * as React from "react";

import { useConversationWorkingAgentPubkeys } from "@/features/agents/agentWorkingSignal";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import type { ChannelPaneProps } from "@/features/channels/ui/ChannelPane.types";
import { parseProjectThreadContext } from "@/features/messages/lib/projectThreadWorkspace";

type BotTypingEntry = ChannelPaneProps["botTypingEntries"][number];

/** Working bot pubkeys for the open thread composer (typing + observer turns). */
export function useThreadComposerBotActivity(
  channelId: string | null | undefined,
  openThreadHeadId: string | null | undefined,
  botTypingEntries: readonly BotTypingEntry[],
  /** When the open thread is a project thread, the sticky bar owns this signal. */
  threadHeadBody?: string | null,
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

  const ownedByStickyBar = parseProjectThreadContext(threadHeadBody) != null;

  return {
    hasThreadComposerBotActivity:
      !ownedByStickyBar && threadComposerWorkingBotPubkeys.length > 0,
    threadComposerConversationId,
    threadComposerWorkingBotPubkeys,
  };
}
