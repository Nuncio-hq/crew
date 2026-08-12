import { toast } from "sonner";

import {
  sessionAgingBannerText,
  type SessionAgingEntry,
} from "@/features/messages/lib/sessionAgingStore";
import {
  blindSessionResetManagedAgent,
  guidedHandoverManagedAgent,
} from "@/shared/api/agentControl";
import { getGlobalAgentConfig } from "@/shared/api/tauriGlobalAgentConfig";
import { subscribeControlResults } from "@/features/agents/observerRelayStore";
import { cn } from "@/shared/lib/cn";

type SessionAgingBannerProps = {
  entries: readonly SessionAgingEntry[];
  agentNamesByPubkey: ReadonlyMap<string, string>;
  rootEventId?: string | null;
  latestOwnerMessage?: string;
  className?: string;
};

export function SessionAgingBanner({
  entries,
  agentNamesByPubkey,
  rootEventId,
  latestOwnerMessage = "",
  className,
}: SessionAgingBannerProps) {
  if (entries.length === 0) {
    return null;
  }

  return (
    <div
      className={cn("flex flex-col gap-1.5", className)}
      data-testid="session-aging-banner"
    >
      {entries.map((entry) => {
        const label =
          entry.agentName ??
          agentNamesByPubkey.get(entry.agentPubkey) ??
          "Agent";
        return (
          <div
            key={`${entry.agentPubkey}:${entry.conversationId}`}
            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-border/50 bg-muted/40 px-3 py-1.5 text-sm text-muted-foreground"
            data-testid={`session-aging-row-${entry.agentPubkey}`}
          >
            <span className="min-w-0 flex-1">
              {sessionAgingBannerText(entry, label)}
            </span>
            <button
              type="button"
              className="shrink-0 text-sm font-medium text-foreground underline-offset-2 hover:underline"
              data-testid={`session-aging-handover-${entry.agentPubkey}`}
              onClick={() => {
                void runGuidedHandover(entry, rootEventId, latestOwnerMessage);
              }}
            >
              New session (guided handover)
            </button>
          </div>
        );
      })}
    </div>
  );
}

async function runGuidedHandover(
  entry: SessionAgingEntry,
  rootEventId: string | null | undefined,
  latestOwnerMessage: string,
) {
  let modelId: string | null = null;
  try {
    const config = await getGlobalAgentConfig();
    modelId = config.handover_summarizer_model?.trim() || null;
  } catch {
    modelId = null;
  }

  const unsubscribe = subscribeControlResults(entry.agentPubkey, (frame) => {
    if (
      frame.type !== "guided_handover" &&
      frame.type !== "blind_session_reset"
    ) {
      return;
    }
    unsubscribe();
    if (frame.status === "ok") {
      toast.success(
        frame.type === "guided_handover"
          ? "Handover note posted — fresh session ready"
          : "Session reset",
      );
      return;
    }
    if (frame.allowBlindReset) {
      toast.error(
        frame.error ??
          "Guided handover failed — you can still reset without a note",
        {
          action: {
            label: "Reset without note",
            onClick: () => {
              void blindSessionResetManagedAgent(
                entry.agentPubkey,
                entry.channelId,
                entry.conversationId,
              ).catch((error) => {
                toast.error(
                  error instanceof Error ? error.message : String(error),
                );
              });
            },
          },
        },
      );
      return;
    }
    toast.error(frame.error ?? `Handover failed (${frame.status})`);
  });

  try {
    await guidedHandoverManagedAgent(
      entry.agentPubkey,
      entry.channelId,
      entry.conversationId,
      {
        modelId,
        rootEventId: rootEventId ?? null,
        latestOwnerMessage,
      },
    );
  } catch (error) {
    unsubscribe();
    toast.error(error instanceof Error ? error.message : String(error));
  }
}
