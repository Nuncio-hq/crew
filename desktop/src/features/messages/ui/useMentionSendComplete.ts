import { buildAgentAddressMentionTags } from "@/features/messages/lib/agentAddressMention.mjs";
import type { PreparedBackgroundLinkPreviews } from "@/features/messages/lib/linkPreviewPreparationStore";
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
} from "@/features/messages/lib/imetaMediaMarkdown";
import type { UseDraftsResult } from "@/features/messages/lib/useDrafts";
import type { UseMentionsResult } from "@/features/messages/lib/useMentions";
import type { UseRichTextEditorResult } from "@/features/messages/lib/useRichTextEditor";
import type { ManagedAgent } from "@/shared/api/types";
import { invokeTauri } from "@/shared/api/tauri";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  getErrorMessage,
  formatMessageSendError,
  resolvePreviewTags,
  dedupeQueuedAgentWakes,
  type QueuedAgentWake,
  mergeOutgoingTagsWithReferenceMentions,
  type PendingNonMemberMentionSend,
  uniqueNormalizedPubkeys,
} from "./useMentionSendFlow.helpers";
import { useComposerViewContext } from "./composerViewContext";
import { useComposerWorkspaceBinding } from "./composerWorkspaceBinding";

type UseMentionSendCompleteOptions = {
  activePreparedLinkPreviews: Set<PreparedBackgroundLinkPreviews>;
  channelIdRef: React.MutableRefObject<string | null>;
  clearComposer: (postSendContent?: string) => void;
  contentRef: React.MutableRefObject<string>;
  drafts: Pick<UseDraftsResult, "loadDraft" | "markDraftSent" | "persistDraft">;
  ensureManagedAgentMentionsReady: (
    mentionPubkeys: string[],
    capturedChannelId: string,
    preparedParticipantPubkeys?: string[],
    preparedManagedAgents?: ManagedAgent[],
  ) => Promise<{
    errors: string[];
    pubkeys: string[];
    agentsToWake: QueuedAgentWake[];
  }>;
  detachedStart: (agent: ManagedAgent, replayFloorUnix?: number) => boolean;
  getManagedAgentsByPubkey: () => Promise<Map<string, ManagedAgent>>;
  hasUnsavedMedia: () => boolean;
  isCompleteSendPendingRef: React.MutableRefObject<boolean>;
  isMountedRef: React.MutableRefObject<boolean>;
  mentions: Pick<
    UseMentionsResult,
    "isAgentPubkey" | "restoreDraftMentionRefs" | "revalidateMentionPubkeys"
  >;
  onAddressedAgentsComposerCleared?: (pubkeys: readonly string[]) => string;
  onAddressedAgentsSendFailed?: (pubkeys: readonly string[]) => void;
  onAddressedAgentsSendSucceeded?: (
    pubkeys: readonly string[],
    newlyPinnedPubkeys: readonly string[],
  ) => void;
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
      forceRest?: boolean,
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
  activePreparedLinkPreviews,
  channelIdRef,
  clearComposer,
  contentRef,
  drafts,
  ensureManagedAgentMentionsReady,
  detachedStart,
  getManagedAgentsByPubkey,
  hasUnsavedMedia,
  isCompleteSendPendingRef,
  isMountedRef,
  mentions,
  onAddressedAgentsComposerCleared,
  onAddressedAgentsSendFailed,
  onAddressedAgentsSendSucceeded,
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
      if (isCompleteSendPendingRef.current) {
        return;
      }
      const sendSignal = draft.preparedLinkPreviews?.signal;
      const isSendCancelled = () => sendSignal?.aborted === true;
      if (isSendCancelled()) return draft.preparedLinkPreviews?.release();
      isCompleteSendPendingRef.current = true;
      setIsCompleteSendPending(true);
      const preparedUpload =
        draft.queuedAttachments.length > 0
          ? prepareBackgroundMediaUpload(draft.queuedAttachments)
          : null;
      const persistPreflightDraft = () => {
        if (isSendCancelled() || !draft.recoveryDraftKey) return;
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
      const persistCanceledDraft = () => {
        if (isSendCancelled() || !draft.recoveryDraftKey) return;
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
      let composerCleared = false;
      let optimisticComposerContent = "";
      const restoreComposerAfterFailure = () => {
        if (!composerCleared) return;
        composerCleared = false;
        persistCanceledDraft();
        const canAnimateCurrentComposer =
          isMountedRef.current &&
          (draft.capturedChannelId === channelIdRef.current ||
            channelIdRef.current === null);
        if (
          canAnimateCurrentComposer &&
          draft.addressedAgentPubkeys.length > 0
        ) {
          onAddressedAgentsSendFailed?.(draft.addressedAgentPubkeys);
        }
        const canRestoreCurrentComposer =
          canAnimateCurrentComposer &&
          contentRef.current.trim() === optimisticComposerContent.trim() &&
          !hasUnsavedMedia();
        if (!canRestoreCurrentComposer && draft.recoveryDraftKey) {
          saveQueuedAttachmentsForDraft(
            draft.recoveryDraftKey,
            draft.queuedAttachments,
          );
        }
        if (!canRestoreCurrentComposer) {
          return;
        }
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
      const clearComposerAfterPreflight = (explicitAgentPubkeys: string[]) => {
        if (
          draft.capturedChannelId === channelIdRef.current ||
          channelIdRef.current === null
        ) {
          clearComposer(resolvePostSendContent?.(explicitAgentPubkeys));
          if (draft.addressedAgentPubkeys.length > 0) {
            optimisticComposerContent =
              onAddressedAgentsComposerCleared?.(draft.addressedAgentPubkeys) ??
              "";
            contentRef.current = optimisticComposerContent;
          }
          composerCleared = true;
        }
      };
      let uploadStarted = false;
      try {
        const admittedMentionPubkeys = uniqueNormalizedPubkeys(
          await mentions.revalidateMentionPubkeys(mentionPubkeys),
        );
        if (isSendCancelled()) return restoreComposerAfterFailure();
        if (!isMountedRef.current) return persistPreflightDraft();
        const admittedMentionPubkeySet = new Set(admittedMentionPubkeys);
        const readyAgentPubkeys = new Set(
          uniqueNormalizedPubkeys(draft.readyAgentPubkeys ?? []).filter(
            (pubkey) => admittedMentionPubkeySet.has(pubkey),
          ),
        );
        const managedAgentsByPubkey = await getManagedAgentsByPubkey();
        if (isSendCancelled()) return restoreComposerAfterFailure();
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
          if (isSendCancelled()) return restoreComposerAfterFailure();
          if (!sendChannelId) {
            return restoreComposerAfterFailure();
          }
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
        // Every wake this send queued: persona creates carried on the draft
        // (enqueued before the non-member prompt could defer us here), then
        // the readiness pass's. Flushed only after the relay accepts the
        // publish — every abort path between here and there just drops them,
        // so no wake (or "your message was sent" failure toast) can exist for
        // a message that never landed. First entry wins the dedupe because it
        // carries the earliest replay floor, and the floor is a lower bound.
        const agentsToWake = dedupeQueuedAgentWakes([
          ...(draft.agentsToWake ?? []),
          ...agentReadiness.agentsToWake,
        ]);
        if (isSendCancelled()) return restoreComposerAfterFailure();
        if (!isMountedRef.current) {
          persistPreflightDraft();
          return;
        }
        if (agentReadiness.errors.length > 0) {
          const message =
            agentReadiness.errors.length === 1
              ? `Could not prepare agent mention: ${agentReadiness.errors[0]}`
              : `Could not prepare agent mentions: ${agentReadiness.errors.join(
                  "; ",
                )}`;
          setNonMemberPromptError(message);
          toast.error(message);
          return restoreComposerAfterFailure();
        }
        if (preparedAgentPubkeys.length > 0 && sendChannelId) {
          try {
            await invokeTauri("sync_agents_to_active_huddle", {
              channelId: sendChannelId,
              agentPubkeys: preparedAgentPubkeys,
            });
            if (isSendCancelled()) return restoreComposerAfterFailure();
          } catch (error) {
            if (isSendCancelled()) return restoreComposerAfterFailure();
            const message = `Could not add mentioned agent to the Huddle: ${getErrorMessage(
              error,
              "Huddle enrollment failed.",
            )}`;
            setNonMemberPromptError(message);
            toast.error(message);
            return restoreComposerAfterFailure();
          }
        }
        const send = onSendRef.current;
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
          // Plain sends have no remaining agent workspace preflight, so the
          // composer can clear while the background preview finishes.
          if (
            clearAfterResolve &&
            !composerCleared &&
            draft.explicitAgentPubkeys.length === 0
          ) {
            clearComposerAfterPreflight([]);
            // Accepted background sends survive composer navigation. The
            // community store still owns cancellation across relay changes.
            if (draft.preparedLinkPreviews) {
              activePreparedLinkPreviews.delete(draft.preparedLinkPreviews);
            }
          }
          const finalOutgoingTags = await resolvePreviewTags(
            draft,
            mediaTags,
            outgoingTags,
          );
          if (!finalOutgoingTags || signal?.aborted || isSendCancelled())
            return restoreComposerAfterFailure();
          // The pass immediately before signing/publish is always fresh:
          // mention authorization is re-validated here unconditionally,
          // whatever did or did not separate it from the admission pass
          // above (#5681).
          const revalidatedMentionPubkeys =
            await mentions.revalidateMentionPubkeys(mentionPubkeys);
          if (signal?.aborted || isSendCancelled()) return;
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
              const message = `Could not resolve Project workspace: ${getErrorMessage(error, "relay lookup failed")}`;
              setNonMemberPromptError(message);
              throw new Error(message, { cause: error });
            }
            finalContent = appendCrewViewAgentContext(
              finalContent,
              viewContext,
            );
          }
          if (signal?.aborted || isSendCancelled()) return;
          // Workspace resolution must succeed before a text draft is cleared.
          // Keep the accepted-send transition ahead of the network await.
          if (clearAfterResolve && !composerCleared) {
            clearComposerAfterPreflight(revalidatedExplicitAgentPubkeys);
          }
          const taskRouting = resolveProjectThreadAgentRouting({
            content: finalContent,
            explicitAgentPubkeys: revalidatedExplicitAgentPubkeys,
            isThreadReply: draft.capturedThreadContext !== null,
            mentionPubkeys: revalidatedMentionPubkeys,
          });
          const finalTagsWithAgentAddress = [
            ...finalOutgoingTags,
            ...buildAgentAddressMentionTags(
              draft.addressedAgentPubkeys,
              taskRouting.mentionPubkeys,
            ),
          ];
          await send(
            finalContent,
            taskRouting.mentionPubkeys,
            mergeOutgoingTagsWithReferenceMentions(
              finalTagsWithAgentAddress,
              taskRouting.referencePubkeys,
            ),
            sendChannelId,
            draft.capturedThreadContext,
            draft.preparedLinkPreviews != null,
          );
          // The relay accepted the publish: flush the queued wakes now,
          // before the post-send cancellation check — a cancellation racing
          // a successful publish must not drop the wake for a message that
          // did land. Fire-and-forget: the send awaits nothing here, and
          // each wake carries its enqueue-time replay floor so the spawned
          // harness replays back past this message however late the flush.
          const notified = new Set(
            taskRouting.mentionPubkeys.map(normalizePubkey),
          );
          for (const wake of agentsToWake) {
            if (notified.has(normalizePubkey(wake.agent.pubkey)))
              detachedStart(wake.agent, wake.replayFloorUnix);
          }
          if (signal?.aborted || isSendCancelled()) return;
          const sentMentionPubkeys = new Set(
            taskRouting.mentionPubkeys.map(normalizePubkey),
          );
          const newlyPinnedPubkeys = draft.inlineAgentMentionPubkeys.filter(
            (pubkey) => sentMentionPubkeys.has(normalizePubkey(pubkey)),
          );
          if (
            draft.capturedChannelId === channelIdRef.current ||
            channelIdRef.current === null
          ) {
            onAddressedAgentsSendSucceeded?.(
              [
                ...new Set([
                  ...draft.addressedAgentPubkeys,
                  ...newlyPinnedPubkeys,
                ]),
              ],
              newlyPinnedPubkeys,
            );
          }
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
          let settleUpload!: () => void;
          const uploadSettled = new Promise<void>((resolve) => {
            settleUpload = resolve;
          });
          uploadStarted = preparedUpload.start({
            onComplete: async (uploaded, signal) => {
              try {
                await finishSend(uploaded, signal);
              } catch (error) {
                restoreComposerAfterFailure();
                toast.error(formatMessageSendError(error));
              } finally {
                settleUpload();
              }
            },
            onError: (error) => {
              restoreComposerAfterFailure();
              toast.error(
                `Upload failed: ${getErrorMessage(error, "Unknown error")}`,
              );
              settleUpload();
            },
            onCancel: () => {
              restoreComposerAfterFailure();
              settleUpload();
            },
          });
          if (!uploadStarted) {
            settleUpload();
            return restoreComposerAfterFailure();
          }
          clearComposerAfterPreflight(draft.explicitAgentPubkeys);
          await uploadSettled;
        }
        if (!preparedUpload) {
          try {
            await finishSend([], undefined, true);
          } catch (error) {
            restoreComposerAfterFailure();
            toast.error(formatMessageSendError(error));
          }
        }
      } catch (error) {
        restoreComposerAfterFailure();
        throw error;
      } finally {
        if (draft.preparedLinkPreviews) {
          activePreparedLinkPreviews.delete(draft.preparedLinkPreviews);
        }
        draft.preparedLinkPreviews?.release();
        if (!uploadStarted) preparedUpload?.cancel();
        isCompleteSendPendingRef.current = false;
        if (isMountedRef.current) {
          setIsCompleteSendPending(false);
        }
      }
    },
    [
      clearComposer,
      contentRef,
      drafts,
      ensureManagedAgentMentionsReady,
      getManagedAgentsByPubkey,
      mentions.isAgentPubkey,
      mentions.revalidateMentionPubkeys,
      onAddressedAgentsComposerCleared,
      onAddressedAgentsSendFailed,
      onAddressedAgentsSendSucceeded,
      onPrepareSendChannel,
      onSendRef,
      richText.setContent,
      setContent,
      detachedStart,
      setPendingImeta,
      restoreQueuedAttachments,
      setSpoileredAttachmentUrls,
      hasUnsavedMedia,
      mentions.restoreDraftMentionRefs,
      activePreparedLinkPreviews,
      workspaceBinding,
      viewContext,
      onSuccessfulExplicitAgentAudience,
      resolvePostSendContent,
      channelIdRef,
      isCompleteSendPendingRef,
      isMountedRef,
      setIsCompleteSendPending,
      setNonMemberPromptError,
    ],
  );
}
