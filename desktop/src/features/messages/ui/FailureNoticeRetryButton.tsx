import * as React from "react";
import { toast } from "sonner";

import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import { describeRetryTurnResult } from "@/features/agents/retryTurnFeedback";
import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { subscribeControlResults } from "@/features/agents/observerRelayStore";
import {
  parseFailureNotice,
  type FailureNotice,
} from "@/features/messages/lib/failureNotice";
import type { TimelineMessage } from "@/features/messages/types";
import { retryManagedAgentTurn } from "@/shared/api/agentControl";
import { normalizePubkey } from "@/shared/lib/pubkey";

function waitForRetryResult(
  agentPubkey: string,
  conversationId: string,
  timeoutMs = 8_000,
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
      if (frame.type !== "retry_turn") return;
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

/**
 * Outer shell: parse tags only (no query hooks). Mounted on every message row;
 * returns null for non-notices so the timeline does not subscribe every row to
 * useManagedAgentsQuery (gotcha #7 / PR #891 class of lag).
 */
export function FailureNoticeRetryButton({
  channelId,
  message,
}: {
  channelId: string | null;
  message: TimelineMessage;
}) {
  const notice = React.useMemo(
    () => parseFailureNotice(message.tags),
    [message.tags],
  );
  if (!channelId || !notice || notice.failedEventIds.length === 0) {
    return null;
  }
  return (
    <FailureNoticeRetryButtonInner
      channelId={channelId}
      message={message}
      notice={notice}
    />
  );
}

function FailureNoticeRetryButtonInner({
  channelId,
  message,
  notice,
}: {
  channelId: string;
  message: TimelineMessage;
  notice: FailureNotice;
}) {
  const managedAgentsQuery = useManagedAgentsQuery();
  const [busy, setBusy] = React.useState(false);

  const agent = React.useMemo(() => {
    if (!message.pubkey) return null;
    const key = normalizePubkey(message.pubkey);
    return (
      (managedAgentsQuery.data ?? []).find(
        (candidate) => normalizePubkey(candidate.pubkey) === key,
      ) ?? null
    );
  }, [managedAgentsQuery.data, message.pubkey]);

  if (!agent) return null;

  // Prefer thread root when present; for top-level failures the notice may
  // reply to the failed event (harness sets that). Never fall back to the
  // notice's own id — that derives a different conversation than the original.
  const rootEventId = message.rootId ?? notice.failedEventIds[0] ?? message.id;
  const conversationId = deriveAgentConversationIdOrNull(
    channelId,
    rootEventId,
  );
  if (!conversationId) return null;

  const handleRetry = async () => {
    setBusy(true);
    try {
      await retryManagedAgentTurn(
        agent.pubkey,
        channelId,
        conversationId,
        notice.failedEventIds,
      );
      const status = await waitForRetryResult(agent.pubkey, conversationId);
      const feedback = describeRetryTurnResult(status, agent.name);
      if (feedback.tone === "success") {
        toast.success(feedback.message);
      } else if (feedback.tone === "info") {
        toast.message(feedback.message);
      } else if (feedback.tone === "error") {
        toast.error(feedback.message);
      } else {
        toast.warning(feedback.message);
      }
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : `Failed to retry with ${agent.name}.`,
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <button
      className="mt-1.5 text-xs font-medium text-muted-foreground underline-offset-2 transition-colors hover:text-foreground hover:underline disabled:opacity-50"
      data-testid={`failure-notice-retry-${message.id}`}
      disabled={busy}
      onClick={() => {
        void handleRetry();
      }}
      type="button"
    >
      {busy ? "Retrying…" : "Retry"}
    </button>
  );
}
