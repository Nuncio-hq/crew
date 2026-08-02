import * as React from "react";
import { toast } from "sonner";

import {
  describeCancelTurnResult,
  pickStrongestCancelTurnStatus,
} from "@/features/agents/cancelTurnFeedback";
import {
  clearPendingAgentRequestsForConversation,
  getPendingAgentRequestsForChannel,
  getPendingAgentRequestsForConversation,
  subscribeMessageEditApplied,
  type PendingAgentRequest,
} from "@/features/agents/dispatchedEventIds";
import {
  getActiveTurnControlTargetsForAgent,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import { subscribeControlResults } from "@/features/agents/observerRelayStore";
import { cancelManagedAgentTurn } from "@/shared/api/agentControl";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { useStableArrayShallow } from "@/shared/hooks/useStableReference";

export type ComposerStopTarget = {
  channelId: string;
  conversationId: string;
  turnId?: string;
};

function waitForCancelResult(
  agentPubkey: string,
  conversationId: string,
  timeoutMs = 5_000,
): Promise<string> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (status: string) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      unsubscribe();
      resolve(status);
    };
    const unsubscribe = subscribeControlResults(agentPubkey, (frame) => {
      if (frame.type !== "cancel_turn") return;
      if (
        typeof frame.conversationId === "string" &&
        frame.conversationId.length > 0 &&
        frame.conversationId !== conversationId
      ) {
        return;
      }
      finish(frame.status);
    });
    const timer = window.setTimeout(() => finish("unconfirmed"), timeoutMs);
  });
}

function targetsForAgent(args: {
  agentPubkey: string;
  channelId: string | null;
  conversationId: string | null;
  pending: readonly PendingAgentRequest[];
}): ComposerStopTarget[] {
  const key = normalizePubkey(args.agentPubkey);
  const turnTargets = getActiveTurnControlTargetsForAgent(key).filter(
    (target) => {
      if (args.conversationId) {
        return target.conversationId === args.conversationId;
      }
      if (args.channelId) {
        return target.channelId === args.channelId;
      }
      return true;
    },
  );
  const pendingTargets = args.pending
    .filter((pending) => pending.agentPubkeys.includes(key))
    .filter((pending) => {
      if (args.conversationId) {
        return pending.conversationId === args.conversationId;
      }
      if (args.channelId) {
        return pending.channelId === args.channelId;
      }
      return true;
    })
    // Queued work has no turn yet, so `turnId` stays absent — that is what
    // routes the cancel to the harness drain path rather than an in-flight
    // signal. Annotated so the merged array below is uniformly typed.
    .map(
      (pending): ComposerStopTarget => ({
        channelId: pending.channelId,
        conversationId: pending.conversationId,
      }),
    );

  const seen = new Set<string>();
  const merged: ComposerStopTarget[] = [];
  for (const target of [...turnTargets, ...pendingTargets]) {
    const id = `${target.channelId}:${target.conversationId}:${target.turnId ?? ""}`;
    if (seen.has(id)) continue;
    seen.add(id);
    merged.push(target);
  }
  return merged;
}

function showCancelFeedback(status: string, agentName: string) {
  const feedback = describeCancelTurnResult(status, agentName);
  if (feedback.tone === "success") {
    toast.success(feedback.message);
  } else if (feedback.tone === "info") {
    toast.message(feedback.message);
  } else if (feedback.tone === "error") {
    toast.error(feedback.message);
  } else {
    toast.warning(feedback.message);
  }
}

/**
 * Stop targets + press handler for the composer activity rail.
 * Includes queued/held requests (no turnId) and in-flight turns.
 */
export function useComposerAgentStop(args: {
  channelId: string | null;
  /** When set, scope to this conversation (thread composer). */
  conversationId?: string | null;
}) {
  const [version, setVersion] = React.useState(0);
  React.useEffect(() => {
    const bump = () => setVersion((current) => current + 1);
    const unsubTurns = subscribeActiveAgentTurns(bump);
    const unsubPending = subscribeMessageEditApplied(bump);
    return () => {
      unsubTurns();
      unsubPending();
    };
  }, []);

  const pending = React.useMemo(() => {
    void version;
    if (args.conversationId) {
      return getPendingAgentRequestsForConversation(args.conversationId);
    }
    return getPendingAgentRequestsForChannel(args.channelId);
  }, [args.channelId, args.conversationId, version]);

  const pendingStable = useStableArrayShallow(pending);

  const getTargetsForAgent = React.useCallback(
    (agentPubkey: string): ComposerStopTarget[] =>
      targetsForAgent({
        agentPubkey,
        channelId: args.channelId,
        conversationId: args.conversationId ?? null,
        pending: pendingStable,
      }),
    [args.channelId, args.conversationId, pendingStable],
  );

  const stopAgent = React.useCallback(
    async (agentPubkey: string, agentName: string) => {
      const targets = getTargetsForAgent(agentPubkey);
      if (targets.length === 0) {
        showCancelFeedback("no_active_turn", agentName);
        return;
      }
      try {
        const statuses = await Promise.all(
          targets.map(async (target) => {
            await cancelManagedAgentTurn(
              agentPubkey,
              target.channelId,
              target.conversationId,
              target.turnId ?? null,
            );
            const status = await waitForCancelResult(
              agentPubkey,
              target.conversationId,
            );
            if (status === "cancelled_queued") {
              clearPendingAgentRequestsForConversation(target.conversationId);
            }
            return status;
          }),
        );
        showCancelFeedback(pickStrongestCancelTurnStatus(statuses), agentName);
      } catch (error) {
        toast.error(
          error instanceof Error
            ? error.message
            : `Failed to stop ${agentName}.`,
        );
      }
    },
    [getTargetsForAgent],
  );

  return {
    getTargetsForAgent,
    hasStoppableWork: (agentPubkey: string) =>
      getTargetsForAgent(agentPubkey).length > 0,
    stopAgent,
  };
}

/** @internal test helper */
export function _targetsForAgentForTest(
  ...args: Parameters<typeof targetsForAgent>
): ReturnType<typeof targetsForAgent> {
  return targetsForAgent(...args);
}
