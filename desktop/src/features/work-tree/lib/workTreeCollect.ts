import {
  isRecentSession,
  isWorkThreadEligible,
  workThreadStatus,
} from "./workTreeEligibility";
import type {
  WorkThreadCiGlyph,
  WorkThreadPrTone,
  WorkThreadRowModel,
} from "./workTreeTypes";

export type CollectWorkspaceSource = {
  branch: string;
  channelId: string | null;
  conversationId: string | null;
  lastActivityAt: number;
  repositoryPath: string | null;
  rootEventId: string;
};

export type CollectRegistrySource = {
  branch: string | null;
  checks: "passing" | "failing" | "pending" | "none";
  lastUsedAt: number | null;
  prDraft: boolean;
  prNumber: number | null;
  prState: string | null;
  repositoryPath: string;
  rootEventId: string;
  routingChannelId?: string | null;
};

export type CollectSessionSource = {
  channelId: string;
  conversationId: string;
  lastSeenAt: number;
  rootEventId: string | null;
  sleeping: boolean;
  title: string | null;
  working: boolean;
};

export type CollectNeedsYouSource = {
  channelId: string;
  conversationId: string;
  createdAt: number;
  rootEventId: string;
};

export function ciGlyphFromChecks(
  checks: CollectRegistrySource["checks"] | null | undefined,
): WorkThreadCiGlyph | null {
  if (checks === "passing") return "pass";
  if (checks === "failing") return "fail";
  if (checks === "pending") return "pending";
  return null;
}

export function prToneFromState(
  state: string | null | undefined,
  isDraft: boolean,
): WorkThreadPrTone | null {
  if (!state) return null;
  const normalized = state.toUpperCase();
  if (normalized === "MERGED") return "merged";
  if (normalized === "CLOSED") return "closed";
  if (isDraft) return "draft";
  return "open";
}

type Acc = {
  branch: string | null;
  channelId: string;
  ciGlyph: WorkThreadCiGlyph | null;
  conversationId: string;
  hasNeedsYou: boolean;
  hasRecentSession: boolean;
  hasWorkspaceBinding: boolean;
  isSleeping: boolean;
  isWorking: boolean;
  lastActivityAt: number;
  prNumber: number | null;
  prTone: WorkThreadPrTone | null;
  threadRootId: string;
  title: string;
  unread: boolean;
};

function emptyAcc(
  threadRootId: string,
  channelId: string,
  conversationId: string,
): Acc {
  return {
    branch: null,
    channelId,
    ciGlyph: null,
    conversationId,
    hasNeedsYou: false,
    hasRecentSession: false,
    hasWorkspaceBinding: false,
    isSleeping: false,
    isWorking: false,
    lastActivityAt: 0,
    prNumber: null,
    prTone: null,
    threadRootId,
    title: "",
    unread: false,
  };
}

function bump(acc: Acc, at: number): void {
  if (at > acc.lastActivityAt) acc.lastActivityAt = at;
}

/**
 * Union workspace, registry, session, and needs-you projections into
 * eligible work-thread rows. Talk-only threads never enter the map.
 */
export function collectWorkThreads(input: {
  channelNameById: ReadonlyMap<string, string>;
  needsYou: readonly CollectNeedsYouSource[];
  now: number;
  registry: readonly CollectRegistrySource[];
  sessions: readonly CollectSessionSource[];
  titlesByConversation?: ReadonlyMap<string, string>;
  titlesByRoot?: ReadonlyMap<string, string>;
  unreadRootIds?: ReadonlySet<string>;
  workspaces: readonly CollectWorkspaceSource[];
}): WorkThreadRowModel[] {
  const byRoot = new Map<string, Acc>();
  const rootByConversation = new Map<string, string>();

  const take = (
    threadRootId: string,
    channelId: string,
    conversationId: string,
  ): Acc => {
    const existing = byRoot.get(threadRootId);
    if (existing) {
      if (conversationId) rootByConversation.set(conversationId, threadRootId);
      return existing;
    }
    const acc = emptyAcc(threadRootId, channelId, conversationId);
    byRoot.set(threadRootId, acc);
    if (conversationId) rootByConversation.set(conversationId, threadRootId);
    return acc;
  };

  for (const request of input.needsYou) {
    const acc = take(
      request.rootEventId,
      request.channelId,
      request.conversationId,
    );
    acc.hasNeedsYou = true;
    bump(acc, request.createdAt);
  }

  for (const session of input.sessions) {
    const rootEventId =
      session.rootEventId ??
      rootByConversation.get(session.conversationId) ??
      null;
    if (!rootEventId) continue;
    const acc = take(rootEventId, session.channelId, session.conversationId);
    const recent = isRecentSession(session.lastSeenAt, input.now);
    acc.hasRecentSession = acc.hasRecentSession || recent;
    acc.isWorking = acc.isWorking || (session.working && !session.sleeping);
    acc.isSleeping = acc.isSleeping || session.sleeping;
    if (session.title && !acc.title) acc.title = session.title;
    bump(acc, session.lastSeenAt);
  }

  for (const workspace of input.workspaces) {
    const existing = byRoot.get(workspace.rootEventId);
    const channelId = existing?.channelId || workspace.channelId;
    if (!channelId) continue;
    const acc = take(
      workspace.rootEventId,
      channelId,
      workspace.conversationId ?? existing?.conversationId ?? "",
    );
    acc.hasWorkspaceBinding = true;
    acc.branch = workspace.branch;
    bump(acc, workspace.lastActivityAt);
  }

  for (const entry of input.registry) {
    if (!entry.rootEventId) continue;
    const existing = byRoot.get(entry.rootEventId);
    const channelId = existing?.channelId || entry.routingChannelId || "";
    if (!channelId) continue;
    const acc = take(
      entry.rootEventId,
      channelId,
      existing?.conversationId ?? "",
    );
    acc.hasWorkspaceBinding = true;
    acc.branch = entry.branch ?? acc.branch;
    acc.prNumber = entry.prNumber ?? acc.prNumber;
    acc.prTone = prToneFromState(entry.prState, entry.prDraft) ?? acc.prTone;
    acc.ciGlyph = ciGlyphFromChecks(entry.checks) ?? acc.ciGlyph;
    if (entry.lastUsedAt) bump(acc, entry.lastUsedAt * 1_000);
  }

  const rows: WorkThreadRowModel[] = [];
  for (const acc of byRoot.values()) {
    if (!acc.channelId) continue;
    if (
      !isWorkThreadEligible({
        hasNeedsYou: acc.hasNeedsYou,
        hasRecentSession: acc.hasRecentSession,
        hasWorkspaceBinding: acc.hasWorkspaceBinding,
      })
    ) {
      continue;
    }
    const title =
      acc.title ||
      input.titlesByRoot?.get(acc.threadRootId) ||
      input.titlesByConversation?.get(acc.conversationId) ||
      "Thread";
    rows.push({
      branch: acc.branch,
      channelId: acc.channelId,
      channelName: input.channelNameById.get(acc.channelId) ?? acc.channelId,
      ciGlyph: acc.ciGlyph,
      conversationId: acc.conversationId,
      hasWorkspaceBinding: acc.hasWorkspaceBinding,
      lastActivityAt: acc.lastActivityAt,
      prNumber: acc.prNumber,
      prTone: acc.prTone,
      status: workThreadStatus({
        hasNeedsYou: acc.hasNeedsYou,
        isSleeping: acc.isSleeping,
        isWorking: acc.isWorking,
      }),
      threadRootId: acc.threadRootId,
      title,
      unread: input.unreadRootIds?.has(acc.threadRootId) ?? false,
    });
  }
  return rows;
}
