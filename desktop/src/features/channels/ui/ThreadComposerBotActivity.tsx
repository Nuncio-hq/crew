import type { ComponentProps } from "react";

import { BotActivityComposerAction } from "@/features/channels/ui/BotActivityBar";
import { useThreadComposerBotActivity } from "@/features/channels/ui/useThreadComposerBotActivity";
import type { ChannelPaneProps } from "@/features/channels/ui/ChannelPane.types";

type ThreadComposerBotActivityProps = {
  agents: ComponentProps<typeof BotActivityComposerAction>["agents"];
  botTypingEntries: ChannelPaneProps["botTypingEntries"];
  channelId: string | null | undefined;
  onOpenAgentSession: ComponentProps<
    typeof BotActivityComposerAction
  >["onOpenAgentSession"];
  openAgentSessionPubkey: ComponentProps<
    typeof BotActivityComposerAction
  >["openAgentSessionPubkey"];
  openThreadHeadId: string | null | undefined;
  profiles: ComponentProps<typeof BotActivityComposerAction>["profiles"];
  threadHeadBody?: string | null;
};

/** Inline bot activity for the open thread composer (conversation-scoped). */
export function ThreadComposerBotActivity({
  agents,
  botTypingEntries,
  channelId,
  onOpenAgentSession,
  openAgentSessionPubkey,
  openThreadHeadId,
  profiles,
  threadHeadBody,
}: ThreadComposerBotActivityProps) {
  const {
    hasThreadComposerBotActivity,
    threadComposerConversationId,
    threadComposerWorkingBotPubkeys,
  } = useThreadComposerBotActivity(
    channelId,
    openThreadHeadId,
    botTypingEntries,
    threadHeadBody,
  );

  if (!hasThreadComposerBotActivity) return null;

  return (
    <BotActivityComposerAction
      agents={agents}
      channelId={channelId ?? null}
      conversationId={threadComposerConversationId}
      onOpenAgentSession={onOpenAgentSession}
      openAgentSessionPubkey={openAgentSessionPubkey}
      profiles={profiles}
      workingBotPubkeys={threadComposerWorkingBotPubkeys}
      variant="inline"
    />
  );
}

export function useThreadComposerBotActivityVisible(
  channelId: string | null | undefined,
  openThreadHeadId: string | null | undefined,
  botTypingEntries: ChannelPaneProps["botTypingEntries"],
  threadHeadBody?: string | null,
) {
  return useThreadComposerBotActivity(
    channelId,
    openThreadHeadId,
    botTypingEntries,
    threadHeadBody,
  ).hasThreadComposerBotActivity;
}
