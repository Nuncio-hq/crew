import * as React from "react";
import { useActiveTurnSummariesForConversation } from "@/features/agents/activeConversationAgentTurnSummaries";
import {
  ThreadSelectedRunControls,
  type ThreadRunSelection,
} from "./ThreadSelectedRunControls";
import {
  threadRunKey,
  useThreadRunControlPublisher,
  type ThreadRunCommitment,
} from "./useThreadRunControlPublisher";

/** Explicit run selection over existing live observer identities and native owner capture. */
export function ThreadActivityRunControls({
  channelId,
  rootEventId,
  conversationId,
  agentPubkey,
}: {
  channelId: string;
  rootEventId: string;
  conversationId: string;
  agentPubkey: string;
}) {
  const publisher = useThreadRunControlPublisher({
    channelId,
    rootEventId,
    conversationId,
    agentPubkey,
  });
  const { epoch, token } = publisher;
  const [selected, setSelected] = React.useState<{
    epoch: typeof epoch;
    run: ThreadRunSelection;
    token: NonNullable<ThreadRunCommitment>["token"];
  } | null>(null);
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  const runs =
    summaries.find((entry) => entry.agentPubkey === agentPubkey)?.runs ?? [];
  const chosen = selected?.epoch === epoch ? selected : null;
  const candidates = [
    ...new Map(
      runs
        .filter((run) => run.sessionId && run.turnId)
        .map((run) => [threadRunKey(run), run]),
    ).values(),
  ];
  const value = chosen ? threadRunKey(chosen.run) : "";
  const stillListed = candidates.some((run) => threadRunKey(run) === value);
  const { publishStop, publishSteer } = publisher.publishersFor(
    chosen ? { run: chosen.run, token: chosen.token } : null,
  );
  if (!publisher.owned) return null;
  return (
    <div className="border-b border-border/60 bg-muted/10 p-2">
      <label className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        Live run
        <select
          aria-label="Activity live run"
          className="min-w-0 flex-1 rounded-md border border-input/60 bg-background px-2 py-1.5 text-sm font-normal text-foreground shadow-xs outline-hidden transition-colors hover:border-input focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50"
          value={value}
          disabled={!token}
          onChange={(event) => {
            const run = candidates.find(
              (candidate) => threadRunKey(candidate) === event.target.value,
            );
            if (!run || !token) {
              setSelected(null);
              return;
            }
            setSelected({
              epoch,
              token,
              run: publisher.buildSelection(run.sessionId, run.turnId),
            });
          }}
        >
          <option value="">Select a live run</option>
          {chosen && !stillListed ? (
            <option value={value}>Selected run finished or unavailable</option>
          ) : null}
          {candidates.map((run) => (
            <option key={threadRunKey(run)} value={threadRunKey(run)}>
              Run {run.turnId.slice(0, 8)}
            </option>
          ))}
        </select>
      </label>
      {publisher.error ? (
        <p
          className="mt-2 rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1.5 text-xs text-destructive"
          role="status"
        >
          {publisher.error}
        </p>
      ) : null}
      {/* Keyed by run identity: a draft or feedback composed for one run can
          never be shown for, or sent to, the run selected after it. The
          wrapper is deliberately not keyed — it owns the captured scope. */}
      <ThreadSelectedRunControls
        key={chosen ? threadRunKey(chosen.run) : "none"}
        selection={chosen?.run ?? null}
        publishStop={publishStop}
        publishSteer={publishSteer}
      />
    </div>
  );
}
