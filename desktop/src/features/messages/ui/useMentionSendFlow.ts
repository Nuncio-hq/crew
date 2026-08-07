import * as React from "react";
import { toast } from "sonner";
import {
  type CreateChannelManagedAgentInput,
  useAttachManagedAgentToChannelMutation,
  useAvailableAcpRuntimes,
  useCreateChannelManagedAgentMutation,
  useManagedAgentsQuery,
  useProvisionChannelManagedAgentMutation,
  useStartManagedAgentMutation,
} from "@/features/agents/hooks";
import { resolvePersonaRuntime } from "@/features/agents/lib/resolvePersonaRuntime";
import { useAddChannelMembersMutation } from "@/features/channels/hooks";
import type { QueuedMediaAttachment } from "@/features/messages/lib/backgroundMediaUploadStore";
import type { UseChannelLinksResult } from "@/features/messages/lib/useChannelLinks";
import type { UseEmojiAutocompleteResult } from "@/features/messages/lib/useEmojiAutocomplete";
import type { ImetaMedia } from "@/features/messages/lib/imetaMediaMarkdown";
import type { UseMentionsResult } from "@/features/messages/lib/useMentions";
import type { UseRichTextEditorResult } from "@/features/messages/lib/useRichTextEditor";
import type { UseDraftsResult } from "@/features/messages/lib/useDrafts";
import type { CustomEmoji } from "@/shared/lib/remarkCustomEmoji";
import type { AcpRuntime, ChannelType, ManagedAgent } from "@/shared/api/types";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { buildCustomEmojiTags } from "@/shared/lib/customEmojiTags";
import {
  getErrorMessage,
  isManagedAgentRunning,
  isProviderBackedAgent,
  type PendingNonMemberMentionSend,
  type SendMessageWithMentionFlowInput,
  uniqueNormalizedPubkeys,
} from "./useMentionSendFlow.helpers";
import { useMentionSendComplete } from "./useMentionSendComplete";
import { useMentionSendInviteHandlers } from "./useMentionSendInviteHandlers";
type UseMentionSendFlowOptions = {
  channelId: string | null;
  channelLinks: Pick<UseChannelLinksResult, "clearChannels">;
  channelType: ChannelType | null;
  contentRef: React.MutableRefObject<string>;
  customEmoji: CustomEmoji[];
  drafts: Pick<UseDraftsResult, "loadDraft" | "markDraftSent" | "persistDraft">;
  emojiAutocomplete: Pick<UseEmojiAutocompleteResult, "clearEmojis">;
  mentions: UseMentionsResult;
  onPrepareSendChannel?: (
    additionalParticipantPubkeys?: string[],
  ) => Promise<string | null>;
  onSendRef: React.MutableRefObject<
    (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
      channelId?: string | null,
      threadContext?: {
        parentEventId: string | null;
        threadHeadId: string | null;
      } | null,
    ) => Promise<void>
  >;
  richText: Pick<
    UseRichTextEditorResult,
    "clearContent" | "setContent" | "setContentAndFocusEnd"
  >;
  setContent: (content: string) => void;
  setIsEmojiPickerOpen: React.Dispatch<React.SetStateAction<boolean>>;
  setPendingImeta: (pendingImeta: ImetaMedia[]) => void;
  hasUnsavedMedia: () => boolean;
  clearQueuedAttachments: () => void;
  restoreQueuedAttachments: (attachments: QueuedMediaAttachment[]) => void;
  setSpoileredAttachmentUrls?: React.Dispatch<
    React.SetStateAction<Set<string>>
  >;
  onSuccessfulExplicitAgentAudience?: (audience: {
    channelId: string;
    expectedGeneration: number;
    expectedRevision: number | null;
    explicitAgentPubkeys: string[];
  }) => void;
  resolvePostSendContent?: (effectiveExplicitAgentPubkeys: string[]) => string;
};
const DM_THREAD_AGENT_MENTION_ERROR =
  "Agents must already be in a DM to be mentioned in its threads. Start a new conversation that includes the agent.";
const DM_THREAD_MEMBERS_LOADING_ERROR =
  "Checking conversation members. Try again in a moment.";
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
  onSendRef,
  richText,
  setContent,
  setIsEmojiPickerOpen,
  setPendingImeta,
  hasUnsavedMedia,
  clearQueuedAttachments,
  restoreQueuedAttachments,
  setSpoileredAttachmentUrls,
  onSuccessfulExplicitAgentAudience,
  resolvePostSendContent,
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
  const previousChannelIdRef = React.useRef(channelId);
  // Tracks the live channel so completeSend can ask "is the user still here?"
  // without being frozen to the compose-time closure.
  const channelIdRef = React.useRef(channelId);
  channelIdRef.current = channelId;
  React.useEffect(() => {
    isMountedRef.current = true;
    return () => {
      isMountedRef.current = false;
    };
  }, []);
  const addMembersMutation = useAddChannelMembersMutation(channelId);
  const attachAgentMutation = useAttachManagedAgentToChannelMutation(channelId);
  const createPersonaAgentMutation =
    useCreateChannelManagedAgentMutation(channelId);
  const provisionPersonaAgentMutation =
    useProvisionChannelManagedAgentMutation(channelId);
  const availableRuntimesQuery = useAvailableAcpRuntimes();
  const managedAgentsQuery = useManagedAgentsQuery();
  const startAgentMutation = useStartManagedAgentMutation();
  const getManagedAgentsByPubkey = React.useCallback(async () => {
    const agents =
      managedAgentsQuery.data ??
      (await managedAgentsQuery.refetch()).data ??
      [];
    return new Map(
      agents.map((agent) => [normalizePubkey(agent.pubkey), agent]),
    );
  }, [managedAgentsQuery.data, managedAgentsQuery.refetch]);
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
  const ensureManagedAgentMentionsReady = React.useCallback(
    async (
      mentionPubkeys: string[],
      capturedChannelId: string,
      preparedParticipantPubkeys: string[] = [],
      preparedManagedAgents: ManagedAgent[] = [],
    ) => {
      if (!capturedChannelId || mentionPubkeys.length === 0) {
        return {
          errors: [] as string[],
          pubkeys: [] as string[],
        };
      }

      const managedAgentsByPubkey = await getManagedAgentsByPubkey();
      for (const agent of preparedManagedAgents) {
        managedAgentsByPubkey.set(normalizePubkey(agent.pubkey), agent);
      }
      const participantPubkeys = new Set([
        ...mentions.memberPubkeys,
        ...preparedParticipantPubkeys.map(normalizePubkey),
      ]);
      const errors: string[] = [];
      const pubkeys: string[] = [];

      for (const pubkey of uniqueNormalizedPubkeys(mentionPubkeys)) {
        const agent = managedAgentsByPubkey.get(pubkey);
        if (!agent) {
          continue;
        }

        try {
          if (participantPubkeys.has(pubkey)) {
            if (isProviderBackedAgent(agent)) {
              if (agent.status !== "deployed") {
                await startAgentMutation.mutateAsync(agent.pubkey);
              }
            } else if (!isManagedAgentRunning(agent)) {
              await startAgentMutation.mutateAsync(agent.pubkey);
            }
          } else {
            await attachAgentMutation.mutateAsync({
              channelId: capturedChannelId,
              agent,
              role: "bot",
            });
          }
          pubkeys.push(pubkey);
        } catch (error) {
          errors.push(
            `${agent.name}: ${getErrorMessage(
              error,
              "Could not prepare agent.",
            )}`,
          );
        }
      }

      return {
        errors,
        pubkeys: uniqueNormalizedPubkeys(pubkeys),
      };
    },
    [
      attachAgentMutation,
      getManagedAgentsByPubkey,
      mentions.memberPubkeys,
      startAgentMutation,
    ],
  );

  const createMentionedPersonaAgents = React.useCallback(
    async (trimmed: string, capturedChannelId: string) => {
      const personaMentions = mentions.extractMentionPersonas(trimmed);
      if (!capturedChannelId || personaMentions.length === 0) {
        return {
          errors: [] as string[],
          agents: [] as ManagedAgent[],
          pubkeys: [] as string[],
        };
      }

      const runtimes = await getAvailableRuntimes();
      const defaultRuntime = runtimes[0] ?? null;
      const errors: string[] = [];
      const agents: ManagedAgent[] = [];
      const pubkeys: string[] = [];
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
      if (postSendContent) {
        richText.setContentAndFocusEnd(postSendContent);
        mentions.cancelMentionAutocomplete();
      } else richText.clearContent();
      setPendingImeta([]);
      clearQueuedAttachments();
      setSpoileredAttachmentUrls?.(new Set());
      if (!postSendContent) mentions.clearMentions();
      channelLinks.clearChannels();
      emojiAutocomplete.clearEmojis();
      setIsEmojiPickerOpen(false);
    },
    [
      channelLinks.clearChannels,
      contentRef,
      emojiAutocomplete.clearEmojis,
      mentions.cancelMentionAutocomplete,
      mentions.clearMentions,
      richText.clearContent,
      richText.setContentAndFocusEnd,
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

  const getNonMemberMentionPubkeys = React.useCallback(
    (pubkeys: string[]) => {
      if (
        channelType === null ||
        channelType === "dm" ||
        !mentions.hasResolvedMembers
      ) {
        return [];
      }

      return uniqueNormalizedPubkeys(pubkeys).filter(
        (pubkey) => !mentions.memberPubkeys.has(pubkey),
      );
    },
    [channelType, mentions.hasResolvedMembers, mentions.memberPubkeys],
  );

  const getDmThreadAgentMentionError = React.useCallback(
    (
      trimmed: string,
      capturedThreadContext: SendMessageWithMentionFlowInput["capturedThreadContext"],
    ) => {
      if (channelType !== "dm" || capturedThreadContext == null) {
        return null;
      }

      if (mentions.extractMentionPersonas(trimmed).length > 0) {
        return DM_THREAD_AGENT_MENTION_ERROR;
      }

      const agentPubkeys = mentions
        .extractMentionPubkeys(trimmed)
        .filter(mentions.isAgentPubkey);
      if (agentPubkeys.length === 0) {
        return null;
      }

      if (!mentions.hasResolvedMembers) {
        return DM_THREAD_MEMBERS_LOADING_ERROR;
      }

      return agentPubkeys.some(
        (pubkey) => !mentions.memberPubkeys.has(normalizePubkey(pubkey)),
      )
        ? DM_THREAD_AGENT_MENTION_ERROR
        : null;
    },
    [
      channelType,
      mentions.extractMentionPersonas,
      mentions.extractMentionPubkeys,
      mentions.hasResolvedMembers,
      mentions.isAgentPubkey,
      mentions.memberPubkeys,
    ],
  );

  const sendMessageWithMentionFlow = React.useCallback(
    async ({
      capturedChannelId,
      capturedThreadContext = null,
      pendingImeta,
      queuedAttachments = [],
      sentDraftKey,
      recoveryDraftKey,
      spoileredAttachmentUrls = new Set(),
      trimmed,
      audienceGeneration = 0,
      audienceRevision = null,
    }: SendMessageWithMentionFlowInput) => {
      if (isMentionSendPendingRef.current) {
        return;
      }

      isMentionSendPendingRef.current = true;
      setIsMentionSendPending(true);
      try {
        const dmThreadAgentMentionError = getDmThreadAgentMentionError(
          trimmed,
          capturedThreadContext,
        );
        if (dmThreadAgentMentionError) {
          setNonMemberPromptError(dmThreadAgentMentionError);
          toast.error(dmThreadAgentMentionError);
          return;
        }

        let effectiveChannelId = capturedChannelId;
        if (!effectiveChannelId && onPrepareSendChannel) {
          effectiveChannelId = await onPrepareSendChannel();
          if (!effectiveChannelId) {
            return;
          }
        }

        const personaMentionResult = await createMentionedPersonaAgents(
          trimmed,
          effectiveChannelId ?? "",
        );
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
        const explicitAgentPubkeys = explicitMentionPubkeys.filter(
          (pubkey) =>
            mentions.isAgentPubkey(pubkey) ||
            createdPersonaAgentPubkeySet.has(pubkey),
        );
        const pubkeys = explicitMentionPubkeys;
        const outgoingTags = buildCustomEmojiTags(trimmed, customEmoji);
        const nonMemberPubkeys = getNonMemberMentionPubkeys(pubkeys);
        let promptNonMemberPubkeys = nonMemberPubkeys.filter(
          (pubkey) =>
            !mentions.isManagedAgentPubkey(pubkey) &&
            !createdPersonaAgentPubkeySet.has(normalizePubkey(pubkey)),
        );

        if (promptNonMemberPubkeys.length > 0) {
          try {
            const managedAgentsByPubkey = await getManagedAgentsByPubkey();
            promptNonMemberPubkeys = promptNonMemberPubkeys.filter(
              (pubkey) => !managedAgentsByPubkey.has(normalizePubkey(pubkey)),
            );
          } catch {
            // Keep the hook-based managed-agent filtering even if the query
            // fallback misses; ordinary non-members still get prompted.
          }
        }

        const pendingDraft: PendingNonMemberMentionSend = {
          capturedChannelId: effectiveChannelId,
          capturedThreadContext,
          trimmed,
          mentionPubkeys: pubkeys,
          nonMemberPubkeys: promptNonMemberPubkeys,
          outgoingTags,
          preparedManagedAgents: personaMentionResult.agents,
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
          savedMentionRefs: mentions.getDraftMentionRefs(trimmed),
          audienceGeneration,
          audienceRevision,
          explicitAgentPubkeys,
        };

        if (promptNonMemberPubkeys.length > 0) {
          setNonMemberPromptError(null);
          setPendingNonMemberSend(pendingDraft);
          return;
        }

        await completeSend(pendingDraft, pubkeys);
      } finally {
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
      getNonMemberMentionPubkeys,
      getDmThreadAgentMentionError,
      mentions.extractMentionPubkeys,
      mentions.isAgentPubkey,
      mentions.isManagedAgentPubkey,
      mentions.getDraftMentionRefs,
      onPrepareSendChannel,
    ],
  );

  const pendingNonMemberNames = React.useMemo(() => {
    if (!pendingNonMemberSend) return [];

    return pendingNonMemberSend.nonMemberPubkeys.map(
      (pubkey) =>
        mentions.getMentionDisplayName(pubkey) ?? truncatePubkey(pubkey),
    );
  }, [mentions.getMentionDisplayName, pendingNonMemberSend]);

  const { handleInviteNonMembers, handleSendWithoutInviting } =
    useMentionSendInviteHandlers({
      addMembersMutation,
      completeSend,
      getManagedAgentsByPubkey,
      mentions,
      pendingNonMemberSend,
      setNonMemberPromptError,
    });

  const dismissNonMemberPrompt = React.useCallback(() => {
    setPendingNonMemberSend(null);
    setNonMemberPromptError(null);
  }, []);

  return {
    dismissNonMemberPrompt,
    isInvitePending:
      isMentionSendPending ||
      isCompleteSendPending ||
      addMembersMutation.isPending ||
      attachAgentMutation.isPending ||
      createPersonaAgentMutation.isPending ||
      startAgentMutation.isPending,
    isPreparingMentionSend:
      isMentionSendPending ||
      isCompleteSendPending ||
      attachAgentMutation.isPending ||
      createPersonaAgentMutation.isPending ||
      startAgentMutation.isPending,
    nonMemberPromptError,
    pendingNonMemberNames,
    pendingNonMemberSend,
    sendMessageWithMentionFlow,
    sendWithoutInviting: handleSendWithoutInviting,
    inviteNonMembers: handleInviteNonMembers,
  };
}
