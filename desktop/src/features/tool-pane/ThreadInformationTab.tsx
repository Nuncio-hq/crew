import * as React from "react";
import { useThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import { useDeclaredPlansForThread } from "@/features/messages/ui/useDeclaredPlansForThread";
import { DeclaredPlansRail } from "@/features/messages/ui/DeclaredPlansRail";
import type { TimelineMessage } from "@/features/messages/types";
import { ThreadAgentTranscript } from "./ThreadAgentTranscript";

const EMPTY_MESSAGES: TimelineMessage[] = [];

/** Read-only lenses over the current conversation and existing D-056 projection. */
export function ThreadInformationTab({
  channelId,
  threadRootId,
  tab,
}: {
  channelId: string;
  threadRootId: string | null;
  tab: "context" | "activity" | "plans";
}) {
  const context = useThreadForgeViewContext();
  const matched =
    context?.channelId === channelId && context.rootEventId === threadRootId
      ? context
      : null;
  if (!matched)
    return (
      <p className="p-3 text-sm text-muted-foreground">
        Thread information unavailable.
      </p>
    );
  if (tab === "context") {
    return (
      <section
        className="space-y-3 overflow-auto p-3"
        aria-label="Thread context"
      >
        <h2 className="text-sm font-semibold">Context</h2>
        <p className="text-sm whitespace-pre-wrap">
          {matched.messages[0]?.body}
        </p>
        <p className="text-xs text-muted-foreground">
          Recap: Off — unavailable.
        </p>
      </section>
    );
  }
  return (
    <ThreadDeclaredInformation
      channelId={channelId}
      context={matched}
      tab={tab}
    />
  );
}

function ThreadDeclaredInformation({
  channelId,
  context,
  tab,
}: {
  channelId: string;
  context: NonNullable<ReturnType<typeof useThreadForgeViewContext>>;
  tab: "activity" | "plans";
}) {
  const messages = React.useMemo(
    () => context.messages.slice(1),
    [context.messages],
  );
  const { plans, conversationId } = useDeclaredPlansForThread({
    channelId,
    threadHead: context.messages[0],
    threadMessages: messages ?? EMPTY_MESSAGES,
    profiles: context.profiles,
  });
  const [selectedAgent, setSelectedAgent] = React.useState<string | null>(null);
  const agent =
    plans.find((plan) => plan.agentPubkey === selectedAgent) ?? plans[0];
  if (tab === "plans") {
    return plans.length > 0 ? (
      <DeclaredPlansRail
        layout="pane"
        plans={plans}
        profiles={context.profiles}
      />
    ) : (
      <p className="p-3 text-sm text-muted-foreground">
        No agent has reported a plan for this thread.
      </p>
    );
  }
  if (!agent || !conversationId)
    return (
      <p className="p-3 text-sm text-muted-foreground">
        No retained activity is available for this thread.
      </p>
    );
  return (
    <section
      className="flex min-h-0 flex-1 flex-col"
      aria-label="Thread activity"
    >
      <label className="flex items-center gap-2 border-b border-border/60 p-2 text-sm">
        Agent
        <select
          aria-label="Activity agent"
          className="min-w-0 flex-1 rounded-md border border-border bg-background p-1"
          value={agent.agentPubkey}
          onChange={(event) => setSelectedAgent(event.target.value)}
        >
          {plans.map((plan) => (
            <option key={plan.agentPubkey} value={plan.agentPubkey}>
              {plan.agentName} · {plan.liveness}
            </option>
          ))}
        </select>
      </label>
      <ThreadAgentTranscript
        agent={agent}
        channelId={channelId}
        conversationId={conversationId}
        key={`${conversationId}:${agent.agentPubkey}`}
        profiles={context.profiles}
      />
    </section>
  );
}
