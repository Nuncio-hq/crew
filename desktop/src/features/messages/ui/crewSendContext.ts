/**
 * Crew-owned pre-publish step for explicit agent sends (D-022 extraction from
 * upstream `useMentionSendFlow.ts`).
 *
 * After upstream has re-validated the mention recipients and immediately
 * before the relay publish, Crew:
 *
 * 1. resolves the Project workspace context for the addressed agents
 *    (fails closed — a resolve error must leave the draft untouched);
 * 2. appends the founder's visible-page context;
 * 3. applies single-agent routing on Project threads (extra agents become
 *    reference tags, not notifications).
 */

import type { WorkspaceBindingChoice } from "@/features/messages/lib/workspaceBindingSpec";
import type { CrewViewContext } from "@/features/projects/lib/project-view-agent-context";
import { appendCrewViewAgentContext } from "@/features/projects/lib/project-view-agent-context";
import { resolveCurrentProjectChannelAgentMessage } from "@/features/projects/lib/project-local-workspace-runtime";
import { filterEffectiveExplicitAgentPubkeys } from "@/features/messages/lib/effectiveExplicitAgentPubkeys";
import { resolveProjectThreadAgentRouting } from "@/features/messages/lib/projectThreadAgentRouting";
import * as React from "react";
import { useComposerViewContext } from "./composerViewContext";
import { useComposerWorkspaceBinding } from "./composerWorkspaceBinding";
import {
  getErrorMessage,
  type PendingNonMemberMentionSend,
} from "./useMentionSendFlow.helpers";

export type CrewSendContext = {
  workspaceBinding: WorkspaceBindingChoice | undefined;
  viewContext: CrewViewContext | null;
};

/** Hook pair read once per flow render; stable object for hook deps. */
export function useCrewSendContext(): CrewSendContext {
  const workspaceBinding = useComposerWorkspaceBinding();
  const viewContext = useComposerViewContext();
  return React.useMemo(
    () => ({ workspaceBinding, viewContext }),
    [workspaceBinding, viewContext],
  );
}

export type CrewSendContextInput = {
  content: string;
  channelId: string;
  draft: Pick<
    PendingNonMemberMentionSend,
    | "addressedAgentPubkeys"
    | "inlineAgentMentionPubkeys"
    | "capturedThreadContext"
  >;
  revalidatedMentionPubkeys: readonly string[];
  /** Surface the resolve failure in the composer before it is thrown. */
  reportError: (message: string) => void;
};

export type CrewSendContextResult = {
  content: string;
  /** Recipients that are notified (may be a subset of the revalidated set). */
  mentionPubkeys: string[];
  /** Deferred agents carried as reference tags only. */
  referencePubkeys: string[];
};

export async function applyCrewSendContext(
  ctx: CrewSendContext,
  input: CrewSendContextInput,
): Promise<CrewSendContextResult> {
  const explicitAgentPubkeys = filterEffectiveExplicitAgentPubkeys(
    [
      ...input.draft.addressedAgentPubkeys,
      ...input.draft.inlineAgentMentionPubkeys,
    ],
    input.revalidatedMentionPubkeys,
  );
  let content = input.content;
  if (explicitAgentPubkeys.length > 0) {
    try {
      content = await resolveCurrentProjectChannelAgentMessage({
        channelId: input.channelId,
        content,
        explicitAgentPubkeys,
        binding: ctx.workspaceBinding,
      });
    } catch (error) {
      const message = `Could not resolve Project workspace: ${getErrorMessage(error, "relay lookup failed")}`;
      input.reportError(message);
      throw new Error(message, { cause: error });
    }
    content = appendCrewViewAgentContext(content, ctx.viewContext);
  }
  const routing = resolveProjectThreadAgentRouting({
    content,
    explicitAgentPubkeys,
    isThreadReply: input.draft.capturedThreadContext !== null,
    mentionPubkeys: [...input.revalidatedMentionPubkeys],
  });
  return {
    content,
    mentionPubkeys: routing.mentionPubkeys,
    referencePubkeys: routing.referencePubkeys,
  };
}
