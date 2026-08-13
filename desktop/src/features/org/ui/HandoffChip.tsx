import * as React from "react";

import {
  getAgentReceipts,
  subscribeAgentReceipts,
} from "@/features/agents/agentReceiptStore";
import { parseHandoffTag } from "@/features/org/lib/handoffTag";
import { displayNameForPubkey } from "@/features/org/lib/orgRoster";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import type { TimelineMessage } from "@/features/messages/types";

export function HandoffChip({ message }: { message: TimelineMessage }) {
  const handoff = parseHandoffTag(message.tags);
  const receipts = React.useSyncExternalStore(
    subscribeAgentReceipts,
    getAgentReceipts,
    getAgentReceipts,
  );
  const profilesQuery = useUsersBatchQuery(handoff ? [handoff.executor] : []);
  if (!handoff) {
    return null;
  }
  const accepted = receipts.some(
    (receipt) =>
      receipt.parentEventId === message.id &&
      receipt.agentPubkey.toLowerCase() === handoff.executor,
  );
  const name = displayNameForPubkey(
    handoff.executor,
    profilesQuery.data?.profiles ?? {},
  );
  return (
    <p
      className="mb-1 text-2xs text-muted-foreground"
      data-testid={`handoff-chip-${message.id}`}
    >
      {accepted ? `✓ accepted by ${name}` : `→ assigned to ${name}`}
    </p>
  );
}
