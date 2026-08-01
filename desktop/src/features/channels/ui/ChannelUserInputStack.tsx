import * as React from "react";
import { ChannelUserInputCard } from "@/features/channels/ui/ChannelUserInputCard";
import type {
  UserInputAnswers,
  UserInputEvent,
} from "@/features/channels/lib/userInput";

type Props = {
  pending: UserInputEvent[];
  sent: UserInputEvent[];
  currentPubkey: string;
  profiles?: Record<string, { ownerPubkey: string | null }>;
  onSkip: (item: UserInputEvent) => Promise<void>;
  onSubmit: (item: UserInputEvent, answers: UserInputAnswers) => Promise<void>;
};

export function ChannelUserInputStack({
  pending,
  sent,
  currentPubkey,
  profiles,
  onSkip,
  onSubmit,
}: Props) {
  const sentIds = React.useMemo(
    () => new Set(sent.map(({ event }) => event.id)),
    [sent],
  );
  return (
    <div
      className="pointer-events-auto mx-5 mb-2 max-h-[min(48vh,34rem)] space-y-2 overflow-y-auto"
      data-testid="channel-user-input-stack"
    >
      {[...sent, ...pending].map((item) => (
        <ChannelUserInputCard
          currentPubkey={currentPubkey}
          item={item}
          key={item.event.id}
          profiles={profiles}
          sent={sentIds.has(item.event.id)}
          onSkip={onSkip}
          onSubmit={onSubmit}
        />
      ))}
    </div>
  );
}
