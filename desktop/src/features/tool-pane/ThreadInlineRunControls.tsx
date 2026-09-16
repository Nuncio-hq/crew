import { useActiveTurnSummariesForConversation } from "@/features/agents/activeConversationAgentTurnSummaries";
import { AGENT_ACTIVITY_CHROME } from "@/features/agents/ui/agentActivityChrome";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { openThreadToolPane } from "./toolPaneStore";
import { ThreadSelectedRunControls } from "./ThreadSelectedRunControls";
import { useThreadRunControlPublisher } from "./useThreadRunControlPublisher";

/** One live run of one agent in this thread. */
type LiveRun = Readonly<{
  agentPubkey: string;
  sessionId: string;
  turnId: string;
  liveness: string | null;
}>;

/** Every live run in a thread, in a stable order across all of its agents. */
export function liveRunsForThread(
  summaries: readonly {
    agentPubkey: string;
    progressLabel?: string | null;
    runs?: readonly { sessionId?: string | null; turnId?: string | null }[];
  }[],
): LiveRun[] {
  const seen = new Set<string>();
  const runs: LiveRun[] = [];
  for (const summary of summaries) {
    for (const run of summary.runs ?? []) {
      if (!run.sessionId || !run.turnId) continue;
      const key = `${summary.agentPubkey}:${run.sessionId}:${run.turnId}`;
      if (seen.has(key)) continue;
      seen.add(key);
      runs.push({
        agentPubkey: summary.agentPubkey,
        sessionId: run.sessionId,
        turnId: run.turnId,
        liveness: summary.progressLabel ?? null,
      });
    }
  }
  return runs;
}

/**
 * Thread-level Steer/Stop, directly above the composer.
 *
 * The Activity tab keeps the full picker; this strip is the fast path for the
 * unambiguous case. It renders nothing for a thread with no live run, and
 * offers direct controls only when exactly one run could be meant — with more
 * than one, choosing for the operator would be guessing, so it points at
 * Activity instead. Controls go through the same publication seam as Activity.
 */
export function ThreadInlineRunControls({
  channelId,
  rootEventId,
  agentNames,
}: {
  channelId: string;
  rootEventId: string;
  /** Display names already resolved by the thread chrome, by pubkey. */
  agentNames?: Readonly<Record<string, string>>;
}) {
  // The controls scope to the same conversation identity Activity uses; a
  // thread that cannot form one has no live run to control.
  const conversationId =
    deriveAgentConversationIdOrNull(channelId, rootEventId) ?? "";
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  const runs = liveRunsForThread(summaries);
  const only = runs.length === 1 ? runs[0] : null;
  if (!conversationId || runs.length === 0) return null;
  return (
    <div
      className="pointer-events-auto flex flex-wrap items-center gap-2 px-5 pb-1.5 text-xs text-muted-foreground"
      data-testid="thread-inline-run-controls"
    >
      {only ? (
        <SingleLiveRunControls
          agentPubkey={only.agentPubkey}
          channelId={channelId}
          conversationId={conversationId}
          agentNames={agentNames}
          liveness={only.liveness}
          rootEventId={rootEventId}
          sessionId={only.sessionId}
          turnId={only.turnId}
        />
      ) : (
        <>
          <span>{AGENT_ACTIVITY_CHROME.runsWorking(runs.length)}</span>
          <button
            className="inline-flex h-7 items-center justify-center rounded-md border border-border bg-background px-2.5 text-xs font-medium text-foreground shadow-xs transition-colors hover:bg-muted/70 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background"
            onClick={() => openThreadToolPane("activity")}
            type="button"
          >
            Open Activity
          </button>
        </>
      )}
    </div>
  );
}

/**
 * The unambiguous case: one live run, bound by its exact identity.
 *
 * Owner capture lives in the shared publisher hook, so this surface cannot
 * publish under a scope Activity would not have accepted.
 */
function SingleLiveRunControls({
  agentNames,
  agentPubkey,
  channelId,
  conversationId,
  liveness,
  rootEventId,
  sessionId,
  turnId,
}: {
  agentNames?: Readonly<Record<string, string>>;
  agentPubkey: string;
  channelId: string;
  conversationId: string;
  liveness: string | null;
  rootEventId: string;
  sessionId: string;
  turnId: string;
}) {
  const publisher = useThreadRunControlPublisher({
    channelId,
    rootEventId,
    conversationId,
    agentPubkey,
  });
  const { token } = publisher;
  const run = publisher.buildSelection(sessionId, turnId);
  const { publishStop, publishSteer } = publisher.publishersFor(
    token ? { run, token } : null,
  );
  const name =
    agentNames?.[agentPubkey] ??
    agentNames?.[normalizePubkey(agentPubkey)] ??
    truncatePubkey(agentPubkey);
  return (
    <>
      <span data-testid="thread-inline-run-agent">
        {name} {AGENT_ACTIVITY_CHROME.isWorking}
      </span>
      {liveness ? (
        <span className="rounded-full border border-border/60 px-1.5 py-0.5 text-2xs uppercase tracking-wide">
          {liveness}
        </span>
      ) : null}
      {publisher.owned && token ? (
        <ThreadSelectedRunControls
          compact
          publishStop={publishStop}
          publishSteer={publishSteer}
          selection={run}
        />
      ) : null}
    </>
  );
}
