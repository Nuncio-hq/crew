import type { ReactNode } from "react";

import { markAgentReceiptReviewed } from "@/features/agents/agentReceiptStore";
import { parseAgentReceipt } from "@/features/messages/lib/agentReceipt.mjs";
import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { AgentReceiptCard } from "./AgentReceiptCard";

export function AgentReceiptMessageBody({
  canToggleReactions,
  currentPubkey,
  fallback,
  message,
  onRequestChanges,
  onReviewed,
  profiles,
  reactionPending,
  reactions,
}: {
  canToggleReactions: boolean;
  currentPubkey?: string;
  fallback: ReactNode;
  message: TimelineMessage;
  onRequestChanges?: () => void;
  onReviewed: () => void | Promise<void>;
  profiles?: UserProfileLookup;
  reactionPending: boolean;
  reactions: readonly TimelineReaction[];
}) {
  const receipt = parseAgentReceipt(message.body);
  if (!receipt) return fallback;

  const receiptOwnerPubkey = message.pubkey
    ? profiles?.[normalizePubkey(message.pubkey)]?.ownerPubkey
    : null;
  const ownedByCurrentUser = Boolean(
    currentPubkey &&
      receiptOwnerPubkey &&
      normalizePubkey(receiptOwnerPubkey) === normalizePubkey(currentPubkey),
  );
  const reviewed =
    ownedByCurrentUser &&
    reactions.some(
      (reaction) =>
        reaction.emoji === "✅" && reaction.reactedByCurrentUser === true,
    );
  const handleReviewed = async () => {
    await onReviewed();
    markAgentReceiptReviewed(message.id);
  };

  return (
    <AgentReceiptCard
      disabled={reactionPending}
      onRequestChanges={onRequestChanges}
      onReviewed={
        ownedByCurrentUser && canToggleReactions ? handleReviewed : undefined
      }
      receipt={receipt}
      reviewed={reviewed}
    />
  );
}
