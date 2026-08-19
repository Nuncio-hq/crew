import * as React from "react";

import { useAppShell } from "@/app/AppShellContext";
import { subscribeActiveAgentTurns } from "@/features/agents/activeAgentTurnsStore";
import type { ActiveConversationTurnSummary } from "@/features/agents/activeConversationTurns";
import {
  getAgentReceipts,
  subscribeAgentReceipts,
} from "@/features/agents/agentReceiptStore";
import {
  findManagedAgentRuntime,
  isManagedAgentRuntimeSleeping,
} from "@/features/agents/managedAgentRuntimeStatus";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import {
  getNeedsYouForAll,
  subscribeNeedsYou,
  type NeedsYouRequest,
} from "@/features/agents/needsYouStore";
import { subscribeAgentObserverStore } from "@/features/agents/observerRelayStore";
import { listProjectThreadWorkspaceSnapshots } from "@/features/agents/projectThreadWorkspaceStore";
import {
  listReadyWorktreeRegistryEntries,
  prefetchProjectWorktreeRegistries,
  subscribeProjectWorktreeRegistry,
} from "@/features/agents/projectWorktreeRegistryStore";
import { useChannelsQuery } from "@/features/channels/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useHomeFeedQuery } from "@/features/home/hooks";
import { buildInboxItems } from "@/features/home/lib/inbox";
import { filterInboxItems } from "@/features/home/lib/inboxViewHelpers";
import { useMissionInboxActiveTurns } from "@/features/home/lib/missionInbox";
import { getThreadReference } from "@/features/messages/lib/threading";
import { useProjectsQuery } from "@/features/projects/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { useNow } from "@/shared/lib/useNow";
import { collectWorkThreads } from "../lib/workTreeCollect";
import {
  getWorkTreeDisclosure,
  subscribeWorkTreeDisclosure,
} from "../lib/workTreeDisclosure";
import {
  buildWorkTreeFolder,
  projectFolderChannelIds,
} from "../lib/workTreeEligibility";
import { aggregateNeedsYou } from "../lib/needsYouAggregation";
import type {
  NeedsYouItem,
  WorkThreadRowModel,
  WorkTreeFolderModel,
} from "../lib/workTreeTypes";

let workTreeEpoch = 0;

function subscribeWorkTreeSources(listener: () => void): () => void {
  const bump = () => {
    workTreeEpoch += 1;
    listener();
  };
  const unsubs = [
    subscribeAgentObserverStore(bump),
    subscribeProjectWorktreeRegistry(bump),
    subscribeNeedsYou(bump),
    subscribeActiveAgentTurns(bump),
    subscribeAgentReceipts(bump),
    subscribeWorkTreeDisclosure(bump),
  ];
  return () => {
    for (const unsub of unsubs) unsub();
  };
}

function sourceGeneration(): number {
  return workTreeEpoch;
}

export function useProjectFolderChannelIds(): ReadonlySet<string> {
  const projectsQuery = useProjectsQuery();
  return React.useMemo(
    () => projectFolderChannelIds(projectsQuery.data ?? []),
    [projectsQuery.data],
  );
}

function linkedRepositoryPaths(
  projects: readonly {
    repositories?: readonly {
      localWorkspacePath?: string | null;
      localWorkspaceStatus?: string;
    }[];
  }[],
): string[] {
  const paths: string[] = [];
  for (const project of projects) {
    for (const repository of project.repositories ?? []) {
      if (
        repository.localWorkspaceStatus === "linked" &&
        repository.localWorkspacePath
      ) {
        paths.push(repository.localWorkspacePath);
      }
    }
  }
  return paths;
}

function sessionRootId(
  turn: ActiveConversationTurnSummary,
  inboxRootByConversation: ReadonlyMap<string, string>,
): string | null {
  return inboxRootByConversation.get(turn.conversationId) ?? null;
}

function workspaceChannelId(input: {
  activeTurns: readonly ActiveConversationTurnSummary[];
  conversationId: string | null;
  inboxItems: readonly {
    conversationId: string;
    item: { channelId: string | null; id: string; tags: string[][] };
  }[];
  rootEventId: string;
}): string | null {
  const needsYou = getNeedsYouForAll();
  if (input.conversationId) {
    return (
      input.activeTurns.find(
        (turn) => turn.conversationId === input.conversationId,
      )?.channelId ??
      needsYou.find(
        (request) => request.conversationId === input.conversationId,
      )?.channelId ??
      input.inboxItems.find(
        (item) => item.conversationId === input.conversationId,
      )?.item.channelId ??
      null
    );
  }
  const fromNeedsYou = needsYou.find(
    (request) => request.rootEventId === input.rootEventId,
  )?.channelId;
  if (fromNeedsYou) return fromNeedsYou;
  for (const item of input.inboxItems) {
    const thread = getThreadReference(item.item.tags);
    const rootId = thread.rootId ?? thread.parentId ?? item.item.id;
    if (rootId === input.rootEventId) return item.item.channelId;
  }
  return null;
}

export function useWorkTreeProjection(unreadChannelIds: ReadonlySet<string>): {
  folders: WorkTreeFolderModel[];
  projectFolderIds: ReadonlySet<string>;
  threads: WorkThreadRowModel[];
} {
  const now = useNow(30_000);
  const channelsQuery = useChannelsQuery();
  const projectsQuery = useProjectsQuery();
  const identityQuery = useIdentityQuery();
  const homeFeedQuery = useHomeFeedQuery();
  const { getChannelReadAt, getMessageReadAt, getThreadReadAt } = useAppShell();
  const channels = channelsQuery.data ?? [];
  const projects = projectsQuery.data ?? [];
  const currentPubkey = identityQuery.data?.pubkey;
  const projectFolderIds = React.useMemo(
    () => projectFolderChannelIds(projects),
    [projects],
  );

  React.useEffect(() => {
    prefetchProjectWorktreeRegistries(linkedRepositoryPaths(projects));
  }, [projects]);

  const getSnapshot = React.useCallback(() => sourceGeneration(), []);
  const generation = useSyncGeneration(getSnapshot);

  const inboxItems = React.useMemo(
    () =>
      filterInboxItems(
        buildInboxItems({
          channels,
          currentPubkey,
          feed: homeFeedQuery.data,
          getChannelReadAt,
          getMessageReadAt,
          getThreadReadAt,
        }),
      ),
    [
      channels,
      currentPubkey,
      getChannelReadAt,
      getMessageReadAt,
      getThreadReadAt,
      homeFeedQuery.data,
    ],
  );

  const titlesByRoot = React.useMemo(() => {
    const titles = new Map<string, string>();
    const conversationRoots = new Map<string, string>();
    for (const item of inboxItems) {
      const thread = getThreadReference(item.item.tags);
      const rootId = thread.rootId ?? thread.parentId ?? item.item.id;
      const title = item.subject || item.preview;
      if (rootId && title && !titles.has(rootId)) titles.set(rootId, title);
      if (item.conversationId && rootId) {
        conversationRoots.set(item.conversationId, rootId);
      }
    }
    return { conversationRoots, titles };
  }, [inboxItems]);

  const unreadRootIds = React.useMemo(() => {
    const unread = new Set<string>();
    for (const item of inboxItems) {
      if (item.unreadCount > 0) {
        const thread = getThreadReference(item.item.tags);
        unread.add(thread.rootId ?? thread.parentId ?? item.item.id);
      }
    }
    return unread;
  }, [inboxItems]);

  const activeTurns = useMissionInboxActiveTurns();
  const { activeCommunity } = useCommunities();
  const activeAgentPubkeys = React.useMemo(
    () => [...new Set(activeTurns.flatMap((turn) => turn.agentPubkeys))],
    [activeTurns],
  );
  const managedAgentRuntimesQuery = useManagedAgentRuntimesQuery({
    enabled: Boolean(
      activeCommunity?.relayUrl && activeAgentPubkeys.length > 0,
    ),
  });
  const sleepingAgentPubkeys = React.useMemo(() => {
    if (!activeCommunity?.relayUrl) return new Set<string>();
    const sleeping = new Set<string>();
    for (const pubkey of activeAgentPubkeys) {
      const runtime = findManagedAgentRuntime(
        managedAgentRuntimesQuery.data ?? [],
        pubkey,
        activeCommunity.relayUrl,
      );
      if (isManagedAgentRuntimeSleeping(runtime)) {
        sleeping.add(normalizePubkey(pubkey));
      }
    }
    return sleeping;
  }, [
    activeAgentPubkeys,
    activeCommunity?.relayUrl,
    managedAgentRuntimesQuery.data,
  ]);

  const channelNameById = React.useMemo(
    () => new Map(channels.map((channel) => [channel.id, channel.name])),
    [channels],
  );

  const threads = React.useMemo(() => {
    void generation;
    const workspaces = listProjectThreadWorkspaceSnapshots().flatMap(
      ({ rootEventId, snapshot }) => {
        if (snapshot.status !== "ready" && snapshot.status !== "derived") {
          return [];
        }
        const conversationId =
          snapshot.status === "ready" ? snapshot.conversationId : null;
        return [
          {
            branch: snapshot.branch,
            channelId: workspaceChannelId({
              activeTurns,
              conversationId,
              inboxItems,
              rootEventId,
            }),
            conversationId,
            lastActivityAt: now,
            repositoryPath: snapshot.repositoryPath,
            rootEventId,
          },
        ];
      },
    );
    const registry = listReadyWorktreeRegistryEntries().flatMap(
      ({ entry, repositoryPath }) => {
        if (!entry.rootEventId) return [];
        const pullRequest = entry.pullRequests[0];
        return [
          {
            branch: entry.branch,
            checks: pullRequest?.checks ?? "none",
            lastUsedAt: entry.lastUsedAt ?? null,
            prDraft: pullRequest?.isDraft ?? false,
            prNumber: pullRequest?.number ?? null,
            prState: pullRequest?.state ?? null,
            repositoryPath,
            rootEventId: entry.rootEventId,
            routingChannelId: entry.routingChannelId ?? null,
          },
        ];
      },
    );
    const sessions = activeTurns.map((turn) => ({
      channelId: turn.channelId,
      conversationId: turn.conversationId,
      lastSeenAt: turn.lastSeenAt,
      rootEventId: sessionRootId(turn, titlesByRoot.conversationRoots),
      sleeping: turn.agentPubkeys.some((pubkey) =>
        sleepingAgentPubkeys.has(normalizePubkey(pubkey)),
      ),
      title: null,
      working: true,
    }));
    const needsYou = getNeedsYouForAll();
    return collectWorkThreads({
      channelNameById,
      needsYou,
      now,
      registry,
      sessions,
      titlesByConversation: new Map(
        inboxItems.map((item) => [
          item.conversationId,
          item.subject || item.preview,
        ]),
      ),
      titlesByRoot: titlesByRoot.titles,
      unreadRootIds,
      workspaces,
    });
  }, [
    activeTurns,
    channelNameById,
    inboxItems,
    now,
    sleepingAgentPubkeys,
    titlesByRoot,
    unreadRootIds,
    generation,
  ]);

  const folders = React.useMemo(() => {
    const byChannel = new Map<string, WorkThreadRowModel[]>();
    for (const thread of threads) {
      if (!projectFolderIds.has(thread.channelId)) continue;
      const bucket = byChannel.get(thread.channelId) ?? [];
      bucket.push(thread);
      byChannel.set(thread.channelId, bucket);
    }
    const folderChannels = channels.filter(
      (channel) =>
        channel.channelType === "stream" && projectFolderIds.has(channel.id),
    );
    return folderChannels.map((channel) =>
      buildWorkTreeFolder({
        channelId: channel.id,
        channelName: channel.name,
        disclosure: getWorkTreeDisclosure(channel.id),
        now,
        threads: byChannel.get(channel.id) ?? [],
        timelineUnread: unreadChannelIds.has(channel.id),
      }),
    );
  }, [channels, now, projectFolderIds, threads, unreadChannelIds]);

  return { folders, projectFolderIds, threads };
}

function useSyncGeneration(
  getSnapshot: () => string | number,
): string | number {
  return React.useSyncExternalStore(
    subscribeWorkTreeSources,
    getSnapshot,
    getSnapshot,
  );
}

export function useNeedsYouItems(): {
  count: number;
  grouped: ReturnType<typeof aggregateNeedsYou>["grouped"];
  items: NeedsYouItem[];
} {
  const now = useNow(30_000);
  const getSnapshot = React.useCallback(() => sourceGeneration(), []);
  const generation = useSyncGeneration(getSnapshot);

  return React.useMemo(() => {
    void generation;
    const requests = getNeedsYouForAll(now);
    const receipts = getAgentReceipts().filter((receipt) => !receipt.reviewed);
    const items: NeedsYouItem[] = [
      ...requests.map((request) => needsYouItemFromRequest(request)),
      ...receipts.flatMap((receipt) => {
        if (!receipt.rootEventId) return [];
        return [
          {
            channelId: receipt.channelId,
            id: receipt.id,
            kind: "evidence" as const,
            threadRootId: receipt.rootEventId,
            title: receipt.summary || "Ready for review",
          },
        ];
      }),
    ];
    return aggregateNeedsYou(items);
  }, [generation, now]);
}

function needsYouItemFromRequest(request: NeedsYouRequest): NeedsYouItem {
  const kind = request.ownerPubkey ? "question" : "approval";
  return {
    channelId: request.channelId,
    id: request.id,
    kind,
    threadRootId: request.rootEventId,
    title: kind === "approval" ? "Approval needed" : "Question",
  };
}
