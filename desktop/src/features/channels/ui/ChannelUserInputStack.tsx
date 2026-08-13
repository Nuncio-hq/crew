import * as React from "react";
import { ChannelUserInputCard } from "@/features/channels/ui/ChannelUserInputCard";
import type {
  UserInputAnswers,
  UserInputEvent,
} from "@/features/channels/lib/userInput";
import {
  OriginApprovalCard,
  isOriginApprovalRequest,
} from "@/features/tool-pane/OriginApprovalCard";

type Props = {
  pending: UserInputEvent[];
  sent: UserInputEvent[];
  resolved: Array<
    UserInputEvent & {
      resolution: "answered" | "declined" | "cancelled";
    }
  >;
  currentPubkey: string;
  profiles?: Record<string, { ownerPubkey: string | null }>;
  errors: Record<string, string>;
  sendingRequestId: string | null;
  onSkip: (item: UserInputEvent) => Promise<void>;
  onSubmit: (item: UserInputEvent, answers: UserInputAnswers) => Promise<void>;
  onDismiss: (requestEventId: string) => void;
};

type ResolvedUserInputEvent = UserInputEvent & {
  resolution: "answered" | "declined" | "cancelled";
};

type CardItem = UserInputEvent | ResolvedUserInputEvent;

export function ChannelUserInputStack({
  pending,
  sent,
  resolved,
  currentPubkey,
  profiles,
  errors,
  sendingRequestId,
  onSkip,
  onSubmit,
  onDismiss,
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
      {([...sent, ...resolved, ...pending] as CardItem[]).map((item) => {
        const resolution = "resolution" in item ? item.resolution : undefined;
        if (isOriginApprovalRequest(item) && !resolution) {
          return (
            <OriginApprovalCard
              item={item}
              key={item.event.id}
              sending={sendingRequestId === item.event.id}
              onSkip={onSkip}
              onSubmit={onSubmit}
            />
          );
        }
        return (
          <ChannelUserInputCard
            currentPubkey={currentPubkey}
            item={item}
            key={item.event.id}
            profiles={profiles}
            sent={sentIds.has(item.event.id)}
            resolution={resolution}
            error={errors[item.event.id]}
            sending={sendingRequestId === item.event.id}
            onSkip={onSkip}
            onSubmit={onSubmit}
            onDismiss={onDismiss}
          />
        );
      })}
    </div>
  );
}
