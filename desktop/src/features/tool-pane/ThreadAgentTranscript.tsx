import * as React from "react";
import type { AgentDeclaredPlan } from "@/features/agents/declaredPlanProjection";
import { AgentSessionTranscriptList } from "@/features/agents/ui/AgentSessionTranscriptList";
import { buildTranscriptState } from "@/features/agents/ui/agentSessionTranscript";
import {
  useArchivedChannelEvents,
  useLoadArchivedObserverEvents,
  useObserverEvents,
} from "@/features/agents/ui/useObserverEvents";
import { mergeProjectThreadPeekEvents } from "@/features/messages/lib/projectThreadMissionControl";
import type { UserProfileLookup } from "@/features/profile/lib/identity";

/** The existing transcript presenter owns expansion and near-end anchored scrolling. */
export function ThreadAgentTranscript({
  agent,
  channelId,
  conversationId,
  profiles,
}: {
  agent: AgentDeclaredPlan;
  channelId: string;
  conversationId: string;
  profiles?: UserProfileLookup;
}) {
  const live = useObserverEvents(true, agent.agentPubkey);
  const archived = useArchivedChannelEvents(agent.agentPubkey, channelId);
  const paging = useLoadArchivedObserverEvents(true, channelId);
  const events = React.useMemo(
    () =>
      mergeProjectThreadPeekEvents(
        live.events,
        archived,
        conversationId,
      ).filter((event) => !event.channelId || event.channelId === channelId),
    [live.events, archived, conversationId, channelId],
  );
  const items = React.useMemo(
    () => buildTranscriptState(events).items,
    [events],
  );
  return (
    <div
      className="flex min-h-0 flex-1 flex-col p-2"
      data-testid="thread-agent-transcript"
    >
      {paging.hasOlderArchived ? (
        <button
          className="shrink-0 p-2 text-xs"
          onClick={() => {
            void paging.fetchOlderArchived();
          }}
          type="button"
        >
          Load older activity
        </button>
      ) : null}
      <AgentSessionTranscriptList
        agentAvatarUrl={profiles?.[agent.agentPubkey]?.avatarUrl ?? null}
        agentName={agent.agentName}
        agentPubkey={agent.agentPubkey}
        autoTail
        channelId={channelId}
        conversationId={conversationId}
        emptyDescription="Transcript unavailable in retained history."
        items={items}
        profiles={profiles}
        scrollScopeKey={`${conversationId}:${agent.agentPubkey}`}
      />
    </div>
  );
}
