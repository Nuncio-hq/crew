import * as React from "react";
import { toast } from "sonner";
import {
  type CreateChannelManagedAgentInput,
  useAttachManagedAgentToChannelMutation,
  useAvailableAcpRuntimes,
  useCreateChannelManagedAgentMutation,
  useManagedAgentsQuery,
  usePersonasQuery,
  useProvisionChannelManagedAgentMutation,
} from "@/features/agents/hooks";
import { resolvePersonaRuntime } from "@/features/agents/lib/resolvePersonaRuntime";
import { useAddChannelMembersMutation } from "@/features/channels/hooks";
import { useCanAddChannelMembers } from "@/features/channels/useCanAddChannelMembers";
import { PRIVATE_CHANNEL_ADD_DENIED_MESSAGE } from "@/features/channels/lib/channelMemberAdmission";
import { dmThreadAgentMentionError } from "@/features/messages/lib/dmThreadAgentMentionError";
import { useActivePreparedLinkPreviews } from "./useActivePreparedLinkPreviews";
import { useDetachedAgentStart } from "./useDetachedAgentStart";
import { useEnsureAgentMentionsReady } from "./useEnsureAgentMentionsReady";
import type { AcpRuntime, ManagedAgent } from "@/shared/api/types";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { buildCustomEmojiTags } from "@/shared/lib/customEmojiTags";
import {
  enqueueAgentWake,
  getErrorMessage,
  mergeMentionRecipients,
  MENTION_REFERENCE_TAG,
  mergeOutgoingTagsWithReferenceMentions,
  type PendingNonMemberMentionSend,
  type QueuedAgentWake,
  type SendMessageWithMentionFlowInput,
  uniqueNormalizedPubkeys,
} from "./useMentionSendFlow.helpers";
import { useMentionSendComplete } from "./useMentionSendComplete";
import type { UseMentionSendFlowOptions } from "./useMentionSendFlow.types";

export function useMentionSendFlow({
  channelId,
  channelLinks,
  channelType,
  contentRef,
  customEmoji,
  drafts,
  emojiAutocomplete,
  mentions,
  onPrepareSendChannel,
  onAddressedAgentsComposerCleared,
  onAddressedAgentsSendFailed,
  onAddressedAgentsSendSucceeded,
  onSuccessfulExplicitAgentAudience,
  resolvePostSendContent,
  onSendRef,
  richText,
  setContent,
  setIsEmojiPickerOpen,
  setPendingImeta,
  hasUnsavedMedia,
  clearQueuedAttachments,
  restoreQueuedAttachments,
  setSpoileredAttachmentUrls,
}: UseMentionSendFlowOptions) {
  const [pendingNonMemberSend, setPendingNonMemberSend] =
    React.useState<PendingNonMemberMentionSend | null>(null);
  const [nonMemberPromptError, setNonMemberPromptError] = React.useState<
    string | null
  >(null);
  const [isMentionSendPending, setIsMentionSendPending] = React.useState(false);
  const [isCompleteSendPending, setIsCompleteSendPending] =
    React.useState(false);
  const isMentionSendPendingRef = React.useRef(false);
  const isCompleteSendPendingRef = React.useRef(false);
  const isMountedRef = React.useRef(false);
  const activePreparedLinkPreviews = useActivePreparedLinkPreviews();
  const previousChannelIdRef = React.useRef(channelId);
  const channelIdRef = React.useRef(channelId);
  channelIdRef.current = channelId;
  React.useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);
  const addMembersMutation = useAddChannelMembersMutation(channelId);
  const canInviteNonMembers = useCanAddChannelMembers(channelId);
  const attachAgentMutation = useAttachManagedAgentToChannelMutation(channelId);
  const createPersonaAgentMutation =
    useCreateChannelManagedAgentMutation(channelId);
  const provisionPersonaAgentMutation =
    useProvisionChannelManagedAgentMutation(channelId);
  const availableRuntimesQuery = useAvailableAcpRuntimes();
  const managedAgentsQuery = useManagedAgentsQuery();
  const personasQuery = usePersonasQuery();
  // Detached (publish-first) agent wake, bound to the community and identity
  // active at this render so a start that outlives a community switch fails
  // closed instead of spawning against the new tenant. The send path never
  // calls it while preparing a message: wakes are queued during preparation
  // and flushed through this callback only after the relay accepts the
  // publish, so a start failure can never toast "your message was sent"
  // before the publish outcome is known.
  const startAgentDetached = useDetachedAgentStart();
  const getManagedAgentsByPubkey = React.useCallback(async () => {
    const agents =
      managedAgentsQuery.data ??
      (await managedAgentsQuery.refetch()).data ??
      [];
    return new Map(
      agents.map((agent) => [normalizePubkey(agent.pubkey), agent]),
    );
  }, [managedAgentsQuery.data, managedAgentsQuery.refetch]);
  const getPersonas = React.useCallback(async () => {
    return personasQuery.data ?? (await personasQuery.refetch()).data ?? [];
  }, [personasQuery.data, personasQuery.refetch]);
  const getAvailableRuntimes = React.useCallback(async (): Promise<
    AcpRuntime[]
  > => {
    const cached = availableRuntimesQuery.data ?? [];
    if (cached.length > 0 || !availableRuntimesQuery.isLoading) {
      return cached;
    }
    const refetched = await availableRuntimesQuery.refetch();
    return (refetched.data ?? []).filter(
      (runtime): runtime is AcpRuntime =>
        runtime.availability === "available" &&
        runtime.command !== null &&
        runtime.binaryPath !== null,
    );
  }, [
    availableRuntimesQuery.data,
    availableRuntimesQuery.isLoading,
    availableRuntimesQuery.refetch,
  ]);
  const ensureManagedAgentMentionsReady = useEnsureAgentMentionsReady({
    attachAgentToChannel: attachAgentMutation.mutateAsync,
    getManagedAgentsByPubkey,
    getPersonas,
    memberPubkeys: mentions.memberPubkeys,
  });
  const createMentionedPersonaAgents = React.useCallback(
    async (trimmed: string, capturedChannelId: string) => {
      const personaMentions = mentions.extractMentionPersonas(trimmed);
      if (!capturedChannelId || personaMentions.length === 0) {
        return {
          errors: [] as string[],
          agents: [] as ManagedAgent[],
          pubkeys: [] as string[],
          agentsToWake: [] as QueuedAgentWake[],
        };
      }
      const runtimes = await getAvailableRuntimes();
      const defaultRuntime = runtimes[0] ?? null;
      const errors: string[] = [];
      const agents: ManagedAgent[] = [];
      const pubkeys: string[] = [];
      // Queued, not fired: the wakes ride the pending draft and flush only
      // after the publish succeeds, so a persona created for a send the
      // non-member prompt later cancels never wakes at all.
      const agentsToWake: QueuedAgentWake[] = [];
      const seenPersonaIds = new Set<string>();
      const shouldProvisionForDm =
        channelType === "dm" && Boolean(onPrepareSendChannel);
      for (const { displayName, persona } of personaMentions) {
        if (seenPersonaIds.has(persona.id)) {
          continue;
        }
        seenPersonaIds.add(persona.id);
        const { runtime } = resolvePersonaRuntime(
          persona.runtime,
          runtimes,
          defaultRuntime,
        );
        if (!runtime) {
          errors.push(`${displayName}: No agent runtime available.`);
          continue;
        }
        try {
          const input: CreateChannelManagedAgentInput & {
            channelId: string;
          } = {
            channelId: capturedChannelId,
            runtime,
            name: persona.displayName,
            personaId: persona.id,
            systemPrompt: persona.systemPrompt,
            avatarUrl: persona.avatarUrl ?? undefined,
            model: persona.model ?? undefined,
            role: "bot",
            ensureRunning: true,
            detachedStart: (agentToWake) =>
              enqueueAgentWake(agentsToWake, agentToWake),
          };
          const result = shouldProvisionForDm
            ? await provisionPersonaAgentMutation.mutateAsync(input)
            : await createPersonaAgentMutation.mutateAsync(input);
          const pubkey = normalizePubkey(result.agent.pubkey);
          agents.push(result.agent);
          pubkeys.push(pubkey);
          mentions.registerMentionPubkey(displayName, pubkey, {
            isAgent: true,
          });
        } catch (error) {
          errors.push(
            `${displayName}: ${getErrorMessage(
              error,
              "Could not create agent.",
            )}`,
          );
        }
      }
      return {
        agents,
        errors,
        pubkeys: uniqueNormalizedPubkeys(pubkeys),
        agentsToWake,
      };
    },
    [
      createPersonaAgentMutation,
      channelType,
      getAvailableRuntimes,
      mentions.extractMentionPersonas,
      mentions.registerMentionPubkey,
      onPrepareSendChannel,
      provisionPersonaAgentMutation,
    ],
  );
  const clearComposer = React.useCallback(
    (postSendContent = "") => {
      setPendingNonMemberSend(null);
      setNonMemberPromptError(null);
      setContent(postSendContent);
      contentRef.current = postSendContent;
      if (postSendContent) richText.setContent(postSendContent);
      else richText.clearContent();
      setPendingImeta([]);
      clearQueuedAttachments();
      setSpoileredAttachmentUrls?.(new Set());
      mentions.clearMentions();
      channelLinks.clearChannels();
      emojiAutocomplete.clearEmojis();
      setIsEmojiPickerOpen(false);
    },
    [
      channelLinks.clearChannels,
      contentRef,
      emojiAutocomplete.clearEmojis,
      mentions.clearMentions,
      richText.clearContent,
      richText.setContent,
      setContent,
      setIsEmojiPickerOpen,
      setPendingImeta,
      clearQueuedAttachments,
      setSpoileredAttachmentUrls,
    ],
  );
  React.useEffect(() => {
    if (previousChannelIdRef.current === channelId) {
      return;
    }
    previousChannelIdRef.current = channelId;
    setPendingNonMemberSend(null);
    setNonMemberPromptError(null);
  }, [channelId]);
  const completeSend = useMentionSendComplete({
    channelIdRef,
    clearComposer,
    contentRef,
    drafts,
    ensureManagedAgentMentionsReady,
    detachedStart: startAgentDetached,
    activePreparedLinkPreviews,
    onAddressedAgentsComposerCleared,
    onAddressedAgentsSendFailed,
    onAddressedAgentsSendSucceeded,
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
  });

  const sendMessageWithMentionFlow = React.useCallback(
    async ({
      addressedAgentPubkeys = [],
      audienceGeneration = 0,
      audienceRevision = null,
      capturedChannelId,
      capturedThreadContext = null,
      pendingImeta,
      queuedAttachments = [],
      linkPreviewTags = [],
      preparedLinkPreviews = null,
      sentDraftKey,
      recoveryDraftKey,
      spoileredAttachmentUrls = new Set(),
      trimmed,
    }: SendMessageWithMentionFlowInput) => {
      if (isMentionSendPendingRef.current) {
        return;
      }
      isMentionSendPendingRef.current = true;
      setIsMentionSendPending(true);
      const isSendCancelled = () =>
        preparedLinkPreviews?.signal.aborted === true;
      let sendPromoted = false;
      if (preparedLinkPreviews) {
        activePreparedLinkPreviews.add(preparedLinkPreviews);
      }
      try {
        if (isSendCancelled()) return;
        const dmThreadAgentMentionErrorMessage = dmThreadAgentMentionError({
          trimmed,
          isThreadReply: capturedThreadContext != null,
          channelType,
          extractMentionPersonas: mentions.extractMentionPersonas,
          extractMentionPubkeys: (text) =>
            mergeMentionRecipients(
              mentions.extractMentionPubkeys(text),
              addressedAgentPubkeys,
            ),
          isAgentPubkey: mentions.isAgentPubkey,
          hasResolvedMembers: mentions.hasResolvedMembers,
          memberPubkeys: mentions.memberPubkeys,
        });
        if (dmThreadAgentMentionErrorMessage) {
          setNonMemberPromptError(dmThreadAgentMentionErrorMessage);
          toast.error(dmThreadAgentMentionErrorMessage);
          return;
        }
        let effectiveChannelId = capturedChannelId;
        if (!effectiveChannelId && onPrepareSendChannel) {
          effectiveChannelId = await onPrepareSendChannel();
          if (isSendCancelled()) return;
          if (!effectiveChannelId) {
            return;
          }
        }
        const personaMentionResult = await createMentionedPersonaAgents(
          trimmed,
          effectiveChannelId ?? "",
        );
        if (isSendCancelled()) return;
        if (personaMentionResult.errors.length > 0) {
          const message =
            personaMentionResult.errors.length === 1
              ? `Could not create agent mention: ${personaMentionResult.errors[0]}`
              : `Could not create agent mentions: ${personaMentionResult.errors.join(
                  "; ",
                )}`;
          setNonMemberPromptError(message);
          toast.error(message);
          return;
        }
        const createdPersonaAgentPubkeys = personaMentionResult.pubkeys;
        const createdPersonaAgentPubkeySet = new Set(
          createdPersonaAgentPubkeys.map(normalizePubkey),
        );
        const explicitMentionPubkeys = uniqueNormalizedPubkeys([
          ...mentions.extractMentionPubkeys(trimmed),
          ...createdPersonaAgentPubkeys,
        ]);
        const pubkeys = mergeMentionRecipients(
          explicitMentionPubkeys,
          addressedAgentPubkeys,
        );
        const outgoingTags = [
          ...buildCustomEmojiTags(trimmed, customEmoji),
          ...linkPreviewTags,
        ];
        const nonMemberPubkeys =
          channelType === null ||
          channelType === "dm" ||
          !mentions.hasResolvedMembers
            ? []
            : uniqueNormalizedPubkeys(pubkeys).filter(
                (pubkey) => !mentions.memberPubkeys.has(pubkey),
              );
        let promptNonMemberPubkeys = nonMemberPubkeys.filter(
          (pubkey) =>
            !mentions.isManagedAgentPubkey(pubkey) &&
            !createdPersonaAgentPubkeySet.has(normalizePubkey(pubkey)),
        );
        if (promptNonMemberPubkeys.length > 0) {
          try {
            const managedAgentsByPubkey = await getManagedAgentsByPubkey();
            if (isSendCancelled()) return;
            promptNonMemberPubkeys = promptNonMemberPubkeys.filter(
              (pubkey) => !managedAgentsByPubkey.has(normalizePubkey(pubkey)),
            );
          } catch {}
        }
        const savedMentionRefs = mentions.getDraftMentionRefs(trimmed);
        const pendingDraft: PendingNonMemberMentionSend = {
          audienceGeneration,
          audienceRevision,
          explicitAgentPubkeys: pubkeys.filter(
            (pubkey) =>
              mentions.isAgentPubkey(pubkey) ||
              createdPersonaAgentPubkeySet.has(pubkey),
          ),
          addressedAgentPubkeys: uniqueNormalizedPubkeys(addressedAgentPubkeys),
          inlineAgentMentionPubkeys: uniqueNormalizedPubkeys(
            savedMentionRefs
              .filter((ref) => ref.isAgent)
              .map((ref) => ref.pubkey),
          ),
          capturedChannelId: effectiveChannelId,
          capturedThreadContext,
          trimmed,
          mentionPubkeys: pubkeys,
          nonMemberPubkeys: promptNonMemberPubkeys,
          outgoingTags,
          preparedLinkPreviews,
          preparedManagedAgents: personaMentionResult.agents,
          agentsToWake: personaMentionResult.agentsToWake,
          readyAgentPubkeys:
            channelType === "dm" && onPrepareSendChannel
              ? []
              : createdPersonaAgentPubkeys,
          savedContent: trimmed,
          savedImeta: [...pendingImeta],
          queuedAttachments: [...queuedAttachments],
          savedSpoileredAttachmentUrls: new Set(spoileredAttachmentUrls),
          sentDraftKey,
          recoveryDraftKey,
          savedMentionRefs,
        };
        if (promptNonMemberPubkeys.length > 0) {
          setNonMemberPromptError(null);
          setPendingNonMemberSend(pendingDraft);
          return;
        }
        sendPromoted = true;
        await completeSend(pendingDraft, pubkeys);
      } finally {
        if (!sendPromoted) {
          if (preparedLinkPreviews) {
            activePreparedLinkPreviews.delete(preparedLinkPreviews);
          }
          preparedLinkPreviews?.release();
        }
        isMentionSendPendingRef.current = false;
        setIsMentionSendPending(false);
      }
    },
    [
      completeSend,
      channelType,
      createMentionedPersonaAgents,
      customEmoji,
      getManagedAgentsByPubkey,
      mentions.extractMentionPersonas,
      mentions.extractMentionPubkeys,
      mentions.hasResolvedMembers,
      mentions.isAgentPubkey,
      mentions.isManagedAgentPubkey,
      mentions.memberPubkeys,
      mentions.getDraftMentionRefs,
      onPrepareSendChannel,
      activePreparedLinkPreviews,
    ],
  );
  const pendingNonMemberNames = React.useMemo(() => {
    if (!pendingNonMemberSend) return [];
    return pendingNonMemberSend.nonMemberPubkeys.map(
      (pubkey) =>
        mentions.getMentionDisplayName(pubkey) ?? truncatePubkey(pubkey),
    );
  }, [mentions.getMentionDisplayName, pendingNonMemberSend]);
  const handleSendWithoutInviting = React.useCallback(() => {
    if (!pendingNonMemberSend) return;
    const nonMemberPubkeys = new Set(
      pendingNonMemberSend.nonMemberPubkeys.map((pubkey) =>
        normalizePubkey(pubkey),
      ),
    );
    const mentionPubkeys = pendingNonMemberSend.mentionPubkeys.filter(
      (pubkey) => !nonMemberPubkeys.has(normalizePubkey(pubkey)),
    );
    const outgoingTags = mergeOutgoingTagsWithReferenceMentions(
      pendingNonMemberSend.outgoingTags,
      nonMemberPubkeys,
    );
    void completeSend(pendingNonMemberSend, mentionPubkeys, outgoingTags);
  }, [completeSend, pendingNonMemberSend]);
  const handleInviteNonMembers = React.useCallback(() => {
    if (!pendingNonMemberSend) return;
    if (!canInviteNonMembers) {
      setNonMemberPromptError(PRIVATE_CHANNEL_ADD_DENIED_MESSAGE);
      return;
    }
    setNonMemberPromptError(null);
    void (async () => {
      const mentionPubkeys = uniqueNormalizedPubkeys(
        await mentions.revalidateMentionPubkeys([
          ...pendingNonMemberSend.mentionPubkeys,
          ...pendingNonMemberSend.nonMemberPubkeys,
        ]),
      );
      const admittedMentionPubkeys = new Set(mentionPubkeys);
      const originalNonMemberPubkeys = new Set(
        pendingNonMemberSend.nonMemberPubkeys.map(normalizePubkey),
      );
      const nonMemberPubkeys = [...originalNonMemberPubkeys].filter(
        admittedMentionPubkeys.has.bind(admittedMentionPubkeys),
      );
      const outgoingTags = (pendingNonMemberSend.outgoingTags ?? []).filter(
        (tag) =>
          tag[0] !== MENTION_REFERENCE_TAG ||
          !originalNonMemberPubkeys.has(normalizePubkey(tag[1] ?? "")),
      );
      const managedAgentsByPubkey = await getManagedAgentsByPubkey();
      if (!isMountedRef.current) return;
      const peoplePubkeys: string[] = [];
      const relayAgentPubkeys: string[] = [];
      for (const pubkey of nonMemberPubkeys) {
        if (managedAgentsByPubkey.has(pubkey)) {
          continue;
        }
        if (mentions.isAgentPubkey(pubkey)) {
          relayAgentPubkeys.push(pubkey);
        } else {
          peoplePubkeys.push(pubkey);
        }
      }
      const errors: string[] = [];
      if (peoplePubkeys.length > 0) {
        const result = await addMembersMutation.mutateAsync({
          channelId: pendingNonMemberSend.capturedChannelId ?? undefined,
          pubkeys: peoplePubkeys,
          role: "member",
        });
        errors.push(...result.errors.map((error) => error.error));
      }
      if (relayAgentPubkeys.length > 0) {
        const result = await addMembersMutation.mutateAsync({
          channelId: pendingNonMemberSend.capturedChannelId ?? undefined,
          pubkeys: relayAgentPubkeys,
          role: "bot",
        });
        errors.push(...result.errors.map((error) => error.error));
      }
      if (errors.length > 0) {
        setNonMemberPromptError(errors.join("; "));
        return;
      }
      await completeSend(
        {
          ...pendingNonMemberSend,
          mentionPubkeys,
          outgoingTags,
        },
        mentionPubkeys,
        outgoingTags,
      );
    })().catch((error) => {
      setNonMemberPromptError(
        error instanceof Error ? error.message : "Could not invite members.",
      );
    });
  }, [
    addMembersMutation,
    canInviteNonMembers,
    completeSend,
    getManagedAgentsByPubkey,
    mentions.isAgentPubkey,
    mentions.revalidateMentionPubkeys,
    pendingNonMemberSend,
  ]);
  const dismissNonMemberPrompt = React.useCallback(() => {
    setPendingNonMemberSend(null);
    setNonMemberPromptError(null);
  }, []);
  return {
    // Agent starts are detached (publish-first), so useDetachedAgentStart's
    // in-flight state deliberately does not gate the composer — a background
    // start must not block the next send.
    isPreparingMentionSend:
      isMentionSendPending ||
      isCompleteSendPending ||
      attachAgentMutation.isPending ||
      createPersonaAgentMutation.isPending,
    nonMemberPromptProps: {
      canInvite: canInviteNonMembers,
      error: nonMemberPromptError,
      isInvitePending:
        isMentionSendPending ||
        isCompleteSendPending ||
        addMembersMutation.isPending ||
        attachAgentMutation.isPending ||
        createPersonaAgentMutation.isPending,
      names: pendingNonMemberNames,
      onDismiss: dismissNonMemberPrompt,
      onDoNothing: handleSendWithoutInviting,
      onInvite: handleInviteNonMembers,
      open: pendingNonMemberSend !== null,
    },
    sendMessageWithMentionFlow,
  };
}
