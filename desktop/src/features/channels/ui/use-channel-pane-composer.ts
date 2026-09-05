import * as React from "react";
import type { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useCardMintJobs } from "@/features/agents/cardMintStore";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import {
  findManagedAgentRuntime,
  isManagedAgentRuntimeWaking,
} from "@/features/agents/managedAgentRuntimeStatus";
import type { MessageTimelineHandle } from "@/features/messages/ui/MessageTimeline";
import type { ChannelPaneProps } from "./ChannelPane.types";
import { mentionsKnownAgent } from "./ChannelPane.helpers";
import { containsWelcomePersonaMention } from "./WelcomeComposerBanner";
import { useChannelComposerBotActivity } from "./useChannelComposerBotActivity";
import { useThreadComposerBotActivityVisible } from "./ThreadComposerBotActivity";

type ComposerInputs = Pick<
  ChannelPaneProps,
  | "activeChannel"
  | "agentPubkeys"
  | "agentSessionAgents"
  | "activityAgents"
  | "activeCommunityRelayUrl"
  | "typingPubkeys"
  | "botTypingEntries"
  | "openThreadHeadId"
  | "onSendMessage"
> & {
  goChannel: ReturnType<typeof useAppNavigation>["goChannel"];
  completeWelcomeComposerBanner: () => void;
  isActiveWelcomeChannel: boolean;
  messageTimelineRef: React.RefObject<MessageTimelineHandle | null>;
};

/** Keep composer activity and post-send navigation scoped to the current mounted channel. */
export function useChannelPaneComposer({
  activeChannel,
  agentPubkeys,
  agentSessionAgents,
  activityAgents = [],
  activeCommunityRelayUrl,
  typingPubkeys,
  botTypingEntries,
  openThreadHeadId,
  onSendMessage,
  goChannel,
  completeWelcomeComposerBanner,
  isActiveWelcomeChannel,
  messageTimelineRef,
}: ComposerInputs) {
  const activeChannelId = activeChannel?.id ?? null;
  const activeChannelIdRef = React.useRef(activeChannelId);
  const channelPaneMountedRef = React.useRef(false);
  activeChannelIdRef.current = activeChannelId;
  React.useEffect(() => {
    channelPaneMountedRef.current = true;
    return () => {
      channelPaneMountedRef.current = false;
    };
  }, []);
  const knownAgentPubkeys = React.useMemo(() => {
    const pubkeys = new Set<string>();
    for (const pubkey of agentPubkeys ?? []) {
      pubkeys.add(pubkey.toLowerCase());
    }
    for (const agent of agentSessionAgents) {
      pubkeys.add(agent.pubkey.toLowerCase());
    }
    for (const agent of activityAgents) {
      pubkeys.add(agent.pubkey.toLowerCase());
    }
    return pubkeys;
  }, [activityAgents, agentPubkeys, agentSessionAgents]);
  const handleSendMessage = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
      channelId?: string | null,
      threadContext?: {
        parentEventId: string | null;
        threadHeadId: string | null;
      } | null,
      forceRest?: boolean,
    ) => {
      const shouldCompleteWelcomeBanner =
        isActiveWelcomeChannel &&
        (containsWelcomePersonaMention(content) ||
          mentionsKnownAgent(mentionPubkeys, knownAgentPubkeys));
      messageTimelineRef.current?.scrollToBottomOnNextUpdate();
      await onSendMessage(
        content,
        mentionPubkeys,
        mediaTags,
        channelId,
        threadContext,
        forceRest,
      );
      if (
        channelId &&
        channelId !== activeChannelId &&
        channelPaneMountedRef.current &&
        activeChannelIdRef.current === activeChannelId
      ) {
        await goChannel(channelId, { replace: true });
      }
      if (shouldCompleteWelcomeBanner) {
        completeWelcomeComposerBanner();
      }
    },
    [
      activeChannelId,
      completeWelcomeComposerBanner,
      goChannel,
      isActiveWelcomeChannel,
      knownAgentPubkeys,
      messageTimelineRef,
      onSendMessage,
    ],
  );
  const hasTypingActivity = typingPubkeys.length > 0;
  // Unified working set for the composer bar: observer-derived turns primary,
  // bot typing fallback (both folded together by agentWorkingSignal). This is
  // what makes the bar show for an agent whose observer stream is live but
  // whose typing signal never arrives — and vice versa.
  const composerWorkingBotPubkeys = useChannelComposerBotActivity(
    activeChannel?.id ?? null,
  );
  const managedAgentRuntimesQuery = useManagedAgentRuntimesQuery({
    enabled: Boolean(activeCommunityRelayUrl && activityAgents.length > 0),
  });
  const wakingBotPubkeys = React.useMemo(() => {
    if (!activeCommunityRelayUrl) return [];
    return activityAgents
      .filter((agent) =>
        isManagedAgentRuntimeWaking(
          findManagedAgentRuntime(
            managedAgentRuntimesQuery.data ?? [],
            agent.pubkey,
            activeCommunityRelayUrl,
          ),
        ),
      )
      .map((agent) => agent.pubkey);
  }, [activeCommunityRelayUrl, activityAgents, managedAgentRuntimesQuery.data]);
  const hasComposerBotActivity = composerWorkingBotPubkeys.length > 0;
  const hasCardMintActivity = useCardMintJobs().length > 0;
  const hasComposerBottomActivity =
    hasComposerBotActivity || hasTypingActivity || hasCardMintActivity;
  const hasThreadComposerBotActivity = useThreadComposerBotActivityVisible(
    activeChannel?.id,
    openThreadHeadId,
    botTypingEntries,
  );
  return {
    handleSendMessage,
    composerWorkingBotPubkeys,
    wakingBotPubkeys,
    hasComposerBottomActivity,
    hasThreadComposerBotActivity,
  };
}
