import * as React from "react";

import { normalizePubkey } from "@/shared/lib/pubkey";
import { useWorkbenchThreadData } from "../hooks/useWorkbenchThreadData";
import type {
  WorkbenchAgentStatus,
  WorkbenchThreadRow,
} from "../lib/workbenchThreadIndex";
import { WorkbenchAgentBar } from "./WorkbenchAgentBar";
import { WorkbenchComposer } from "./WorkbenchComposer";
import { WorkbenchHeader } from "./WorkbenchHeader";
import { WorkbenchOfficeBar } from "./WorkbenchOfficeBar";
import { WorkbenchTranscript } from "./WorkbenchTranscript";

export function WorkbenchThreadView({
  channelId,
  officeView,
  onOpenChannel,
  onToggleOffice,
  railRow,
  threadRootId,
}: {
  channelId: string;
  officeView: boolean;
  onOpenChannel: () => void;
  onToggleOffice: () => void;
  railRow: WorkbenchThreadRow | null;
  threadRootId: string;
}) {
  const data = useWorkbenchThreadData(channelId, threadRootId);
  const title =
    railRow?.title ?? data.threadHead?.body?.slice(0, 80) ?? "Thread";
  const sentIds = data.userInput.sentRequestIds;
  const statusByPubkey = React.useMemo(() => {
    const map = new Map<string, WorkbenchAgentStatus>(data.statusByPubkey);
    for (const agent of railRow?.agents ?? []) {
      const key = normalizePubkey(agent.pubkey);
      if (key && !map.has(key)) map.set(key, agent.status);
    }
    return map;
  }, [data.statusByPubkey, railRow?.agents]);

  if (!data.threadHead) {
    return (
      <div
        className="flex flex-1 items-center justify-center text-sm text-muted-foreground"
        data-testid="workbench-thread-loading"
      >
        Loading thread…
      </div>
    );
  }

  return (
    <section
      className="flex min-h-0 min-w-0 flex-1 flex-col"
      data-testid="workbench-thread"
    >
      <WorkbenchHeader
        channelId={channelId}
        channelName={railRow?.channelName ?? data.channel?.name ?? channelId}
        conversationId={data.conversationId}
        officeView={officeView}
        onOpenChannel={onOpenChannel}
        onToggleOffice={onToggleOffice}
        profiles={data.profiles}
        rootEventId={threadRootId}
        title={title}
        workspaceModel={data.workspaceModel}
      />
      {officeView ? <WorkbenchOfficeBar /> : null}
      <WorkbenchAgentBar
        agents={data.agents}
        officeView={officeView}
        statusByPubkey={statusByPubkey}
        targetPubkey={data.targetPubkey}
      />
      <WorkbenchTranscript
        channelId={channelId}
        currentPubkey={data.currentPubkey}
        errors={data.userInput.errors}
        officeView={officeView}
        onSkip={data.userInput.skip}
        onSubmit={data.userInput.answer}
        onToggleReaction={data.onToggleReaction}
        profiles={data.profiles}
        rows={data.transcriptRows}
        sendingRequestId={data.userInput.sendingRequestId}
        sentIds={sentIds}
        threadHeadId={threadRootId}
      />
      <WorkbenchComposer
        agents={data.agents}
        channelId={channelId}
        channelName={railRow?.channelName ?? data.channel?.name ?? channelId}
        conversationId={data.conversationId}
        isSending={data.isSending}
        officeView={officeView}
        onSend={data.send}
        onTargetChange={data.setTargetPubkey}
        profiles={data.profiles}
        targetPubkey={data.targetPubkey}
        threadRootId={threadRootId}
      />
    </section>
  );
}
