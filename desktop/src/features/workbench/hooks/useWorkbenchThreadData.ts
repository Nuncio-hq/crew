import * as React from "react";
import { useQuery } from "@tanstack/react-query";

import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useManagedAgentObserverBridge } from "@/features/agents/observerRelayStore";
import {
  findManagedAgentRuntime,
  isManagedAgentRuntimeSleeping,
  managedAgentWakingStatusLabel,
} from "@/features/agents/managedAgentRuntimeStatus";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useChannelUserInput } from "@/features/channels/hooks/useChannelUserInput";
import {
  useChannelMembersQuery,
  useChannelsQuery,
} from "@/features/channels/hooks";
import {
  useChannelSubscription,
  useChannelWindowQuery,
  useSendMessageMutation,
  useToggleReactionMutation,
} from "@/features/messages/hooks";
import { formatTimelineMessages } from "@/features/messages/lib/formatTimelineMessages";
import { useThreadReplies } from "@/features/messages/useThreadReplies";
import { useProjectThreadWorkspaceModel } from "@/features/messages/ui/useProjectThreadWorkspaceModel";
import { useRelaySelfQuery } from "@/features/moderation/hooks";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { getEventById } from "@/shared/api/tauri";
import { collectThreadAgents } from "../lib/workbenchAgents";
import {
  defaultComposerTarget,
  lastInteractingAgentPubkey,
} from "../lib/workbenchComposerTarget";
import { firstUnreadAfterReadAt } from "../lib/workbenchCatchUp";
import {
  buildWorkbenchTranscript,
  type WorkbenchSleepWake,
} from "../lib/workbenchTranscript";
import { useWorkbenchObserverBundles } from "./useWorkbenchObserverBundles";
import { useAppShell } from "@/app/AppShellContext";
import type { ProjectThreadAgentMention } from "@/features/messages/lib/projectThreadWorkspace";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { WorkbenchAgentStatus } from "../lib/workbenchThreadIndex";

export function useWorkbenchThreadData(
  channelId: string | undefined,
  threadRootId: string | undefined,
) {
  const identityQuery = useIdentityQuery();
  const currentPubkey = identityQuery.data?.pubkey;
  const channels = useChannelsQuery().data ?? [];
  const channel = React.useMemo(
    () => channels.find((candidate) => candidate.id === channelId) ?? null,
    [channelId, channels],
  );
  useChannelSubscription(channel);
  useChannelWindowQuery(channel);
  const repliesQuery = useThreadReplies(channel, threadRootId ?? null);
  const headQuery = useQuery({
    enabled: Boolean(threadRootId),
    queryKey: ["workbench-thread-head", threadRootId],
    queryFn: () => getEventById(threadRootId ?? ""),
  });
  const members = useChannelMembersQuery(channelId ?? null).data ?? [];
  const events = React.useMemo(() => {
    const replies = repliesQuery.data ?? [];
    const head = headQuery.data;
    if (!head) return replies;
    if (replies.some((event) => event.id === head.id)) return replies;
    return [head, ...replies];
  }, [headQuery.data, repliesQuery.data]);
  const eventPubkeys = React.useMemo(
    () => [...new Set(events.map((event) => event.pubkey))],
    [events],
  );
  const profiles = useUsersBatchQuery(eventPubkeys, {
    enabled: eventPubkeys.length > 0,
  }).data?.profiles;
  const relaySelf = useRelaySelfQuery().data;
  const messages = React.useMemo(
    () =>
      formatTimelineMessages(
        events,
        channel,
        currentPubkey,
        null,
        profiles,
        members,
        undefined,
        undefined,
        relaySelf,
      ),
    [channel, currentPubkey, events, members, profiles, relaySelf],
  );
  const threadHead =
    messages.find((message) => message.id === threadRootId) ??
    messages[0] ??
    null;
  const knownAgentPubkeys = useKnownAgentPubkeys();
  const managedAgents = useManagedAgentsQuery({
    enabled: Boolean(currentPubkey),
  }).data;
  useManagedAgentObserverBridge(managedAgents ?? []);
  const agents = React.useMemo(
    () =>
      collectThreadAgents({
        knownAgentPubkeys,
        managedAgents: managedAgents ?? [],
        messages,
        profiles,
      }),
    [knownAgentPubkeys, managedAgents, messages, profiles],
  );
  const lastInteracting = React.useMemo(
    () =>
      lastInteractingAgentPubkey(
        messages,
        new Set(agents.map((agent) => agent.pubkey)),
      ),
    [agents, messages],
  );
  const [targetPubkey, setTargetPubkey] = React.useState<string | null>(null);
  React.useEffect(() => {
    setTargetPubkey(defaultComposerTarget(agents, lastInteracting));
  }, [agents, lastInteracting]);

  const userInput = useChannelUserInput(channelId ?? null);
  const conversationId = deriveAgentConversationIdOrNull(
    channelId,
    threadRootId,
  );
  const observerByAgent = useWorkbenchObserverBundles(
    agents.map((agent) => agent.pubkey),
  );
  const { activeCommunity } = useCommunities();
  const runtimesQuery = useManagedAgentRuntimesQuery({
    enabled: Boolean(activeCommunity?.relayUrl && agents.length > 0),
  });
  const sleepWake = React.useMemo(() => {
    if (!activeCommunity?.relayUrl) return [];
    const lines: WorkbenchSleepWake[] = [];
    for (const agent of agents) {
      const runtime = findManagedAgentRuntime(
        runtimesQuery.data ?? [],
        agent.pubkey,
        activeCommunity.relayUrl,
      );
      if (isManagedAgentRuntimeSleeping(runtime)) {
        lines.push({
          agentPubkey: agent.pubkey,
          kind: "sleep" as const,
          label: `${agent.name} is sleeping · wakes on mention`,
        });
      }
      if (runtime?.lifecycle === "waking") {
        lines.push({
          agentPubkey: agent.pubkey,
          kind: "wake" as const,
          label: managedAgentWakingStatusLabel(agent.name),
        });
      }
    }
    return lines;
  }, [activeCommunity?.relayUrl, agents, runtimesQuery.data]);
  const statusByPubkey = React.useMemo(() => {
    const map = new Map<string, WorkbenchAgentStatus>();
    for (const line of sleepWake) {
      map.set(
        normalizePubkey(line.agentPubkey),
        line.kind === "sleep" ? "sleeping" : "working",
      );
    }
    return map;
  }, [sleepWake]);
  const { getThreadReadAt, markThreadRead } = useAppShell();
  const openedKey = `${channelId ?? ""}:${threadRootId ?? ""}`;
  const openedKeyRef = React.useRef<string | null>(null);
  const [readAtOnOpen, setReadAtOnOpen] = React.useState<number | null>(null);
  React.useEffect(() => {
    if (!threadRootId) return;
    if (openedKeyRef.current === openedKey) return;
    openedKeyRef.current = openedKey;
    setReadAtOnOpen(getThreadReadAt(threadRootId, channelId));
  }, [channelId, getThreadReadAt, openedKey, threadRootId]);
  const catchUpAfterId = React.useMemo(
    () => firstUnreadAfterReadAt(messages, readAtOnOpen),
    [messages, readAtOnOpen],
  );
  React.useEffect(() => {
    if (!threadRootId || messages.length === 0) return;
    const latest = Math.max(...messages.map((message) => message.createdAt));
    markThreadRead(threadRootId, latest);
  }, [markThreadRead, messages, threadRootId]);

  const transcriptRows = React.useMemo(() => {
    if (!channelId || !threadRootId) return [];
    return buildWorkbenchTranscript({
      channelId,
      catchUpAfterId,
      conversationId,
      messages,
      observerByAgent,
      sleepWake,
      threadRootId,
      userInputs: [
        ...userInput.pending,
        ...userInput.sent,
        ...userInput.resolved,
      ],
    });
  }, [
    catchUpAfterId,
    channelId,
    conversationId,
    messages,
    observerByAgent,
    sleepWake,
    threadRootId,
    userInput.pending,
    userInput.resolved,
    userInput.sent,
  ]);

  const agentMentions = React.useMemo<ProjectThreadAgentMention[]>(
    () =>
      agents.map((agent) => ({
        pubkey: agent.pubkey,
        source: "root",
      })),
    [agents],
  );
  const workspaceModel = useProjectThreadWorkspaceModel({
    agentMentions,
    profiles,
    replies: messages.filter((message) => message.id !== threadHead?.id),
    threadHead,
  });
  const sendMessageMutation = useSendMessageMutation(
    channel,
    identityQuery.data,
  );
  const toggleReactionMutation = useToggleReactionMutation();

  const send = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
    ) => {
      if (!threadHead) return;
      await sendMessageMutation.mutateAsync({
        content,
        mentionPubkeys,
        mediaTags,
        parentEventId: threadHead.id,
        threadHeadId: threadHead.id,
      });
    },
    [sendMessageMutation, threadHead],
  );

  const onToggleReaction = React.useCallback(
    async (message: { id: string }, emoji: string, remove: boolean) => {
      if (!channelId) return;
      await toggleReactionMutation.mutateAsync({
        eventId: message.id,
        emoji,
        remove,
      });
    },
    [channelId, toggleReactionMutation],
  );

  return {
    agents,
    channel,
    conversationId,
    currentPubkey: currentPubkey ?? "",
    profiles,
    send,
    setTargetPubkey,
    statusByPubkey,
    targetPubkey,
    threadHead,
    transcriptRows,
    userInput,
    workspaceModel,
    isSending: sendMessageMutation.isPending,
    onToggleReaction,
  };
}
