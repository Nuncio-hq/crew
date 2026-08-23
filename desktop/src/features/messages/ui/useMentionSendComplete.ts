import * as React from "react";
import { toast } from "sonner";

import { resolveCurrentProjectChannelAgentMessage } from "@/features/projects/lib/project-local-workspace-runtime";
import { appendCrewViewAgentContext } from "@/features/projects/lib/project-view-agent-context";
import { filterEffectiveExplicitAgentPubkeys } from "@/features/messages/lib/effectiveExplicitAgentPubkeys";
import { resolveProjectThreadAgentRouting } from "@/features/messages/lib/projectThreadAgentRouting";
import {
  prepareBackgroundMediaUpload,
  saveQueuedAttachmentsForDraft,
  type QueuedMediaAttachment,
} from "@/features/messages/lib/backgroundMediaUploadStore";
import {
  buildOutgoingMessage,
  type ImetaMedia,
  mergeOutgoingTags,
} from "@/features/messages/lib/imetaMediaMarkdown";
import type { UseDraftsResult } from "@/features/messages/lib/useDrafts";
import type { UseMentionsResult } from "@/features/messages/lib/useMentions";
import type { UseRichTextEditorResult } from "@/features/messages/lib/useRichTextEditor";
import type { ManagedAgent } from "@/shared/api/types";
import { invokeTauri } from "@/shared/api/tauri";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  getErrorMessage,
  mergeOutgoingTagsWithReferenceMentions,
  type PendingNonMemberMentionSend,
  uniqueNormalizedPubkeys,
} from "./useMentionSendFlow.helpers";
import { useComposerViewContext } from "./composerViewContext";
import { useComposerWorkspaceBinding } from "./composerWorkspaceBinding";

type UseMentionSendCompleteOptions = {
  channelIdRef: React.MutableRefObject<string | null>;
  clearComposer: (postSendContent?: string) => void;
  contentRef: React.MutableRefObject<string>;
  drafts: Pick<UseDraftsResult, "loadDraft" | "markDraftSent" | "persistDraft">;
  ensureManagedAgentMentionsReady: (
    mentionPubkeys: string[],
    capturedChannelId: string,
    preparedParticipantPubkeys?: string[],
    preparedManagedAgents?: ManagedAgent[],
  ) => Promise<{ errors: string[]; pubkeys: string[] }>;
  getManagedAgentsByPubkey: () => Promise<Map<string, ManagedAgent>>;
  hasUnsavedMedia: () => boolean;
  isCompleteSendPendingRef: React.MutableRefObject<boolean>;
  isMountedRef: React.MutableRefObject<boolean>;
  mentions: Pick<
    UseMentionsResult,
    "isAgentPubkey" | "restoreDraftMentionRefs" | "revalidateMentionPubkeys"
  >;
  onPrepareSendChannel?: (
    additionalParticipantPubkeys?: string[],
  ) => Promise<string | null>;
  onSendRef: React.MutableRefObject<
    (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
      channelId?: string | null,
      threadContext?: PendingNonMemberMentionSend["capturedThreadContext"],
    ) => Promise<void>
  >;
  onSuccessfulExplicitAgentAudience?: (audience: {
    channelId: string;
    expectedGeneration: number;
    expectedRevision: number | null;
    explicitAgentPubkeys: string[];
  }) => void;
  resolvePostSendContent?: (effectiveExplicitAgentPubkeys: string[]) => string;
  restoreQueuedAttachments: (attachments: QueuedMediaAttachment[]) => void;
  richText: Pick<UseRichTextEditorResult, "setContent">;
  setContent: (content: string) => void;
  setIsCompleteSendPending: React.Dispatch<React.SetStateAction<boolean>>;
  setNonMemberPromptError: React.Dispatch<React.SetStateAction<string | null>>;
  setPendingImeta: (pendingImeta: ImetaMedia[]) => void;
  setSpoileredAttachmentUrls?: React.Dispatch<
    React.SetStateAction<Set<string>>
  >;
};

export function useMentionSendComplete({
  channelIdRef,
  clearComposer,
  contentRef,
  drafts,
  ensureManagedAgentMentionsReady,
  getManagedAgentsByPubkey,
  hasUnsavedMedia,
  isCompleteSendPendingRef,
  isMountedRef,
  mentions,
  onPrepareSendChannel,
  onSendRef,
  onSuccessfulExplicitAgentAudience,
  resolvePostSendContent,
  restoreQueuedAttachments,
  richText,
  setContent,
  setIsCompleteSendPending,
  setNonMemberPromptError,
  setPendingImeta,
  setSpoileredAttachmentUrls,
}: UseMentionSendCompleteOptions) {
  const workspaceBinding = useComposerWorkspaceBinding();
  const viewContext = useComposerViewContext();
  return React.useCallback(
    async (
      draft: PendingNonMemberMentionSend,
      mentionPubkeys: string[],
      outgoingTags = draft.outgoingTags,
    ) => {
      if (isCompleteSendPendingRef.current) return;

      isCompleteSendPendingRef.current = true;
      setIsCompleteSendPending(true);
      const preparedUpload =
        draft.queuedAttachments.length > 0
          ? prepareBackgroundMediaUpload(draft.queuedAttachments)
          : null;
      const persistPreflightDraft = () => {
        if (!draft.recoveryDraftKey) return;
        drafts.persistDraft(
          draft.recoveryDraftKey,
          draft.savedContent,
          draft.capturedChannelId ?? draft.recoveryDraftKey,
          draft.savedImeta,
          [...draft.savedSpoileredAttachmentUrls],
          draft.savedMentionRefs,
        );
        saveQueuedAttachmentsForDraft(
          draft.recoveryDraftKey,
          draft.queuedAttachments,
        );
      };
      let uploadStarted = false;
      try {
        const admittedMentionPubkeys = uniqueNormalizedPubkeys(
          await mentions.revalidateMentionPubkeys(mentionPubkeys),
        );
        if (!isMountedRef.current) return persistPreflightDraft();
        const admittedMentionPubkeySet = new Set(admittedMentionPubkeys);
        const readyAgentPubkeys = new Set(
          uniqueNormalizedPubkeys(draft.readyAgentPubkeys ?? []).filter(
            (pubkey) => admittedMentionPubkeySet.has(pubkey),
          ),
        );
        const managedAgentsByPubkey = await getManagedAgentsByPubkey();
        if (!isMountedRef.current) {
          persistPreflightDraft();
          return;
        }
        for (const agent of draft.preparedManagedAgents ?? []) {
          managedAgentsByPubkey.set(normalizePubkey(agent.pubkey), agent);
        }
        const normalizedMentionPubkeys = admittedMentionPubkeys;
        const managedMentionPubkeys = normalizedMentionPubkeys.filter(
          (pubkey) => managedAgentsByPubkey.has(pubkey),
        );
        const agentMentionPubkeys = uniqueNormalizedPubkeys([
          ...managedMentionPubkeys,
          ...normalizedMentionPubkeys.filter(mentions.isAgentPubkey),
        ]);
        const preparedAgentPubkeys = uniqueNormalizedPubkeys([
          ...readyAgentPubkeys,
          ...agentMentionPubkeys,
        ]);
        let sendChannelId = draft.capturedChannelId;
        if (preparedAgentPubkeys.length > 0 && onPrepareSendChannel) {
          sendChannelId = await onPrepareSendChannel(preparedAgentPubkeys);
          if (!sendChannelId) return;
          if (!isMountedRef.current) {
            persistPreflightDraft();
            return;
          }
        }

        const agentReadiness = await ensureManagedAgentMentionsReady(
          managedMentionPubkeys.filter(
            (pubkey) => !readyAgentPubkeys.has(normalizePubkey(pubkey)),
          ),
          sendChannelId ?? "",
          onPrepareSendChannel ? preparedAgentPubkeys : [],
          [...managedAgentsByPubkey.values()],
        );
        if (!isMountedRef.current) {
          persistPreflightDraft();
          return;
        }
        if (agentReadiness.errors.length > 0) {
          const message =
            agentReadiness.errors.length === 1
              ? `Could not start agent mention: ${agentReadiness.errors[0]}`
              : `Could not start agent mentions: ${agentReadiness.errors.join(
                  "; ",
                )}`;
          setNonMemberPromptError(message);
          toast.error(message);
          return;
        }

        if (preparedAgentPubkeys.length > 0 && sendChannelId) {
          try {
            await invokeTauri("sync_agents_to_active_huddle", {
              channelId: sendChannelId,
              agentPubkeys: preparedAgentPubkeys,
            });
          } catch (error) {
            const message = `Could not add mentioned agent to the Huddle: ${getErrorMessage(
              error,
              "Huddle enrollment failed.",
            )}`;
            setNonMemberPromptError(message);
            toast.error(message);
            return;
          }
        }

        const effectiveExplicitAgentPubkeys =
          filterEffectiveExplicitAgentPubkeys(
            draft.explicitAgentPubkeys,
            mentionPubkeys,
          );
        const send = onSendRef.current;
        const persistCanceledDraft = () => {
          if (!draft.recoveryDraftKey) return;
          const existing = drafts.loadDraft(draft.recoveryDraftKey);
          if (
            existing &&
            (existing.content !== draft.savedContent ||
              existing.channelId !==
                (draft.capturedChannelId ?? draft.recoveryDraftKey) ||
              JSON.stringify(existing.pendingImeta) !==
                JSON.stringify(draft.savedImeta) ||
              JSON.stringify(existing.spoileredAttachmentUrls) !==
                JSON.stringify([...draft.savedSpoileredAttachmentUrls]))
          ) {
            return;
          }
          drafts.persistDraft(
            draft.recoveryDraftKey,
            draft.savedContent,
            draft.capturedChannelId ?? draft.recoveryDraftKey,
            draft.savedImeta,
            [...draft.savedSpoileredAttachmentUrls],
            draft.savedMentionRefs,
          );
        };
        const restoreComposerAfterFailure = () => {
          persistCanceledDraft();
          const canRestoreCurrentComposer =
            isMountedRef.current &&
            (draft.capturedChannelId === channelIdRef.current ||
              channelIdRef.current === null) &&
            contentRef.current.trim().length === 0 &&
            !hasUnsavedMedia();
          if (!canRestoreCurrentComposer && draft.recoveryDraftKey) {
            saveQueuedAttachmentsForDraft(
              draft.recoveryDraftKey,
              draft.queuedAttachments,
            );
          }
          if (!canRestoreCurrentComposer) return;
          setContent(draft.savedContent);
          contentRef.current = draft.savedContent;
          richText.setContent(draft.savedContent);
          setPendingImeta(draft.savedImeta);
          restoreQueuedAttachments(draft.queuedAttachments);
          mentions.restoreDraftMentionRefs(draft.savedMentionRefs);
          setSpoileredAttachmentUrls?.(
            new Set(draft.savedSpoileredAttachmentUrls),
          );
        };
        const finishSend = async (
          uploaded: ImetaMedia[],
          signal?: AbortSignal,
          clearAfterResolve = false,
        ) => {
          const { content: builtContent, mediaTags } = buildOutgoingMessage(
            draft.trimmed,
            [...draft.savedImeta, ...uploaded],
            new Set([
              ...draft.savedSpoileredAttachmentUrls,
              ...draft.queuedAttachments.flatMap((attachment, index) =>
                attachment.spoilered && uploaded[index]
                  ? [uploaded[index].url]
                  : [],
              ),
            ]),
          );
          const finalOutgoingTags = mergeOutgoingTags(
            mediaTags,
            outgoingTags ?? [],
          );
          if (signal?.aborted) return;
          // Re-check mention authorization immediately before the network send:
          // a directory or ownership change during upload/resolve must drop the
          // agent mention rather than ride along on the published event.
          const revalidatedMentionPubkeys =
            await mentions.revalidateMentionPubkeys(mentionPubkeys);
          if (signal?.aborted) return;
          const revalidatedExplicitAgentPubkeys =
            filterEffectiveExplicitAgentPubkeys(
              draft.explicitAgentPubkeys,
              revalidatedMentionPubkeys,
            );
          let finalContent = builtContent;
          if (revalidatedExplicitAgentPubkeys.length > 0) {
            try {
              finalContent = await resolveCurrentProjectChannelAgentMessage({
                channelId: sendChannelId ?? draft.capturedChannelId ?? "",
                content: finalContent,
                explicitAgentPubkeys: revalidatedExplicitAgentPubkeys,
                binding: workspaceBinding,
              });
            } catch (error) {
              const message = `Could not resolve Project workspace: ${getErrorMessage(
                error,
                "relay lookup failed",
              )}`;
              setNonMemberPromptError(message);
              toast.error(message);
              throw error instanceof Error ? error : new Error(message);
            }
            // Visible-page context is agent-only and additive: it never
            // replaces the workspace context the harness parses.
            finalContent = appendCrewViewAgentContext(
              finalContent,
              viewContext,
            );
          }
          // No-upload: clear after resolve succeeds, before the network send, so
          // persistent audiences transition atomically while a failed Project
          // lookup never wipes the draft (throw above skips this clear).
          if (
            clearAfterResolve &&
            (draft.capturedChannelId === channelIdRef.current ||
              channelIdRef.current === null)
          ) {
            clearComposer(
              resolvePostSendContent?.(revalidatedExplicitAgentPubkeys),
            );
          }
          const taskRouting = resolveProjectThreadAgentRouting({
            content: finalContent,
            explicitAgentPubkeys: revalidatedExplicitAgentPubkeys,
            isThreadReply: draft.capturedThreadContext !== null,
            mentionPubkeys: revalidatedMentionPubkeys,
          });
          const routedOutgoingTags = mergeOutgoingTagsWithReferenceMentions(
            finalOutgoingTags,
            taskRouting.referencePubkeys,
          );
          await send(
            finalContent,
            taskRouting.mentionPubkeys,
            routedOutgoingTags,
            sendChannelId,
            draft.capturedThreadContext,
          );
          if (signal?.aborted) return;
          if (revalidatedExplicitAgentPubkeys.length > 0) {
            onSuccessfulExplicitAgentAudience?.({
              channelId: sendChannelId ?? draft.capturedChannelId ?? "",
              expectedGeneration: draft.audienceGeneration,
              expectedRevision: draft.audienceRevision,
              explicitAgentPubkeys: revalidatedExplicitAgentPubkeys,
            });
          }
          if (draft.sentDraftKey) {
            drafts.markDraftSent(
              draft.sentDraftKey,
              draft.savedContent,
              draft.capturedChannelId ?? draft.sentDraftKey,
              draft.savedImeta,
              [...draft.savedSpoileredAttachmentUrls],
            );
          }
        };
        if (preparedUpload) {
          uploadStarted = preparedUpload.start({
            onComplete: async (uploaded, signal) => {
              try {
                await finishSend(uploaded, signal);
              } catch {
                restoreComposerAfterFailure();
              }
            },
            onError: (error) => {
              restoreComposerAfterFailure();
              toast.error(
                `Upload failed: ${getErrorMessage(error, "Unknown error")}`,
              );
            },
            onCancel: () => {
              restoreComposerAfterFailure();
            },
          });
          if (!uploadStarted) return;
        }

        if (preparedUpload) {
          // Uploads clear optimistically at start; finishSend restores on failure.
          if (
            draft.capturedChannelId === channelIdRef.current ||
            channelIdRef.current === null
          ) {
            clearComposer(
              resolvePostSendContent?.(effectiveExplicitAgentPubkeys),
            );
          }
        } else {
          try {
            // Clear inside finishSend after resolve, before network send.
            await finishSend([], undefined, true);
          } catch {
            restoreComposerAfterFailure();
            return;
          }
        }
      } finally {
        if (!uploadStarted) preparedUpload?.cancel();
        isCompleteSendPendingRef.current = false;
        if (isMountedRef.current) setIsCompleteSendPending(false);
      }
    },
    [
      channelIdRef,
      clearComposer,
      contentRef,
      drafts,
      ensureManagedAgentMentionsReady,
      getManagedAgentsByPubkey,
      hasUnsavedMedia,
      isCompleteSendPendingRef,
      isMountedRef,
      mentions.isAgentPubkey,
      mentions.restoreDraftMentionRefs,
      mentions.revalidateMentionPubkeys,
      onPrepareSendChannel,
      onSendRef,
      onSuccessfulExplicitAgentAudience,
      resolvePostSendContent,
      restoreQueuedAttachments,
      richText.setContent,
      setContent,
      setIsCompleteSendPending,
      setNonMemberPromptError,
      setPendingImeta,
      setSpoileredAttachmentUrls,
      viewContext,
      workspaceBinding,
    ],
  );
}
