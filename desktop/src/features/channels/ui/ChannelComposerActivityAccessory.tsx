import type { ComponentProps } from "react";

import { CardMintComposerChip } from "@/features/agents/ui/CardMintComposerChip";
import { useCardMintJobs } from "@/features/agents/cardMintStore";
import { BotActivityComposerAction } from "@/features/channels/ui/BotActivityBar";
import { ComposerActivityAccessory } from "@/features/messages/ui/ComposerActivityAccessory";
import { TypingIndicatorRow } from "@/features/messages/ui/TypingIndicatorRow";

type ChannelComposerActivityAccessoryProps = {
  agents: ComponentProps<typeof BotActivityComposerAction>["agents"];
  channel: ComponentProps<typeof TypingIndicatorRow>["channel"];
  conversationId?: ComponentProps<
    typeof BotActivityComposerAction
  >["conversationId"];
  currentPubkey: ComponentProps<typeof TypingIndicatorRow>["currentPubkey"];
  onOpenAgentSession: ComponentProps<
    typeof BotActivityComposerAction
  >["onOpenAgentSession"];
  openAgentSessionPubkey: ComponentProps<
    typeof BotActivityComposerAction
  >["openAgentSessionPubkey"];
  profiles: ComponentProps<typeof BotActivityComposerAction>["profiles"];
  typingPubkeys: string[];
  visible: boolean;
  wakingBotPubkeys: string[];
  workingBotPubkeys: string[];
};

export function ChannelComposerActivityAccessory({
  agents,
  channel,
  conversationId = null,
  currentPubkey,
  onOpenAgentSession,
  openAgentSessionPubkey,
  profiles,
  typingPubkeys,
  visible,
  wakingBotPubkeys,
  workingBotPubkeys,
}: ChannelComposerActivityAccessoryProps) {
  const cardMintJobs = useCardMintJobs();
  return (
    <ComposerActivityAccessory
      className="px-5"
      testId="channel-composer-activity-row"
      visible={visible}
    >
      <div className="flex w-full items-center gap-2 overflow-visible pl-2">
        {cardMintJobs.length > 0 ? <CardMintComposerChip /> : null}
        {workingBotPubkeys.length > 0 ? (
          <div className="flex min-w-0 flex-1 overflow-visible">
            <BotActivityComposerAction
              agents={agents}
              channelId={channel?.id ?? null}
              conversationId={conversationId}
              onOpenAgentSession={onOpenAgentSession}
              openAgentSessionPubkey={openAgentSessionPubkey}
              profiles={profiles}
              wakingBotPubkeys={wakingBotPubkeys}
              workingBotPubkeys={workingBotPubkeys}
              variant="inline"
            />
          </div>
        ) : null}
        {typingPubkeys.length > 0 ? (
          <TypingIndicatorRow
            channel={channel}
            className="min-w-0 flex-1 py-0 pl-[calc(0.75rem+1px)] pr-0 [@container(min-width:40rem)]:pl-[calc(1rem+1px)]"
            currentPubkey={currentPubkey}
            profiles={profiles}
            typingPubkeys={typingPubkeys}
          />
        ) : null}
      </div>
    </ComposerActivityAccessory>
  );
}
