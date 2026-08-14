import {
  WORK_TREE_FOLDER_CAP,
  WORK_TREE_QUIET_MS,
  type WorkThreadRowModel,
  type WorkThreadStatus,
  type WorkTreeDisclosure,
  type WorkTreeFolderBadge,
  type WorkTreeFolderModel,
} from "./workTreeTypes";

export function isWorkThreadEligible(input: {
  hasNeedsYou: boolean;
  hasRecentSession: boolean;
  hasWorkspaceBinding: boolean;
}): boolean {
  return (
    input.hasWorkspaceBinding || input.hasRecentSession || input.hasNeedsYou
  );
}

export function isRecentSession(lastSeenAt: number, now: number): boolean {
  return now - lastSeenAt <= WORK_TREE_QUIET_MS;
}

export function workThreadStatus(input: {
  hasNeedsYou: boolean;
  isSleeping: boolean;
  isWorking: boolean;
}): WorkThreadStatus {
  if (input.hasNeedsYou) return "needs-you";
  if (input.isWorking && !input.isSleeping) return "working";
  return "sleeping";
}

export function folderBadge(
  threads: readonly WorkThreadRowModel[],
): WorkTreeFolderBadge | null {
  if (threads.some((thread) => thread.status === "needs-you")) {
    return { kind: "needs-you" };
  }
  const live = threads.filter((thread) => thread.status === "working").length;
  if (live === 0) return null;
  return { kind: "live", count: live };
}

export function shouldAutoCollapse(input: {
  lastActivityAt: number;
  now: number;
  pinned: boolean;
}): boolean {
  if (input.pinned) return false;
  return input.now - input.lastActivityAt >= WORK_TREE_QUIET_MS;
}

export function capThreads(
  threads: readonly WorkThreadRowModel[],
  cap = WORK_TREE_FOLDER_CAP,
  moreExpanded = false,
): { hiddenCount: number; visible: WorkThreadRowModel[] } {
  if (moreExpanded || threads.length <= cap) {
    return { hiddenCount: 0, visible: [...threads] };
  }
  return {
    hiddenCount: threads.length - cap,
    visible: threads.slice(0, cap),
  };
}

export function sortWorkThreads(
  threads: readonly WorkThreadRowModel[],
): WorkThreadRowModel[] {
  return [...threads].sort((left, right) => {
    if (right.lastActivityAt !== left.lastActivityAt) {
      return right.lastActivityAt - left.lastActivityAt;
    }
    return left.threadRootId.localeCompare(right.threadRootId);
  });
}

export function applyCollapsedArrival(input: {
  disclosure: WorkTreeDisclosure | undefined;
  lastActivityAt: number;
  now: number;
}): { expanded: boolean; pinned: boolean } {
  const pinned = input.disclosure?.pinned === true;
  const autoCollapsed = shouldAutoCollapse({
    lastActivityAt: input.lastActivityAt,
    now: input.now,
    pinned,
  });
  if (input.disclosure?.expanded === false) {
    return { expanded: false, pinned };
  }
  if (input.disclosure?.expanded === true) {
    return { expanded: true, pinned };
  }
  return { expanded: !autoCollapsed, pinned };
}

/**
 * Live updates never auto-expand a collapsed folder. The badge/count
 * changes; disclosure stays where the user (or auto-collapse) left it.
 */
export function disclosureAfterLiveArrival(
  current: WorkTreeDisclosure | undefined,
): WorkTreeDisclosure {
  return {
    expanded: current?.expanded ?? false,
    moreExpanded: current?.moreExpanded,
    pinned: current?.pinned,
  };
}

export function buildWorkTreeFolder(input: {
  channelId: string;
  channelName: string;
  disclosure?: WorkTreeDisclosure;
  now: number;
  threads: readonly WorkThreadRowModel[];
  timelineUnread: boolean;
}): WorkTreeFolderModel {
  const threads = sortWorkThreads(input.threads);
  const lastActivityAt = threads[0]?.lastActivityAt ?? Number.NEGATIVE_INFINITY;
  const { expanded, pinned } = applyCollapsedArrival({
    disclosure: input.disclosure,
    lastActivityAt: Number.isFinite(lastActivityAt)
      ? lastActivityAt
      : input.now,
    now: input.now,
  });
  const autoCollapsed =
    !expanded &&
    shouldAutoCollapse({
      lastActivityAt: Number.isFinite(lastActivityAt)
        ? lastActivityAt
        : input.now,
      now: input.now,
      pinned,
    });
  const { hiddenCount, visible } = expanded
    ? capThreads(threads, WORK_TREE_FOLDER_CAP, input.disclosure?.moreExpanded)
    : { hiddenCount: 0, visible: [] };
  return {
    autoCollapsed,
    badge: folderBadge(threads),
    channelId: input.channelId,
    channelName: input.channelName,
    expanded,
    hiddenCount,
    lastActivityAt: Number.isFinite(lastActivityAt) ? lastActivityAt : 0,
    pinned,
    timelineUnread: input.timelineUnread,
    threads,
    visibleThreads: visible,
  };
}

/**
 * A channel is a project folder when it is the exclusive repo/project
 * binding. Shared access channels (many repos pointing at #general) stay
 * Slack rows.
 */
export function projectFolderChannelIds(
  projects: readonly {
    projectChannelId?: string | null;
    repositories?: readonly {
      channelId?: string | null;
      repoAddress?: string;
    }[];
  }[],
): Set<string> {
  const fromProject = new Set<string>();
  const repoByChannel = new Map<string, Set<string>>();
  for (const project of projects) {
    if (project.projectChannelId) {
      fromProject.add(project.projectChannelId);
    }
    for (const repository of project.repositories ?? []) {
      const channelId = repository.channelId;
      if (!channelId) continue;
      const address = repository.repoAddress ?? channelId;
      const bucket = repoByChannel.get(channelId) ?? new Set();
      bucket.add(address);
      repoByChannel.set(channelId, bucket);
    }
  }
  const folders = new Set(fromProject);
  for (const [channelId, repos] of repoByChannel) {
    if (repos.size === 1) folders.add(channelId);
  }
  return folders;
}
