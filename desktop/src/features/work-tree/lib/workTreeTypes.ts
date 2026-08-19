/** Compact status shown on a work-thread row (issue #203). */
export type WorkThreadStatus = "working" | "sleeping" | "needs-you";

export type WorkThreadCiGlyph = "pass" | "fail" | "pending";

export type WorkThreadPrTone = "open" | "draft" | "merged" | "closed";

/**
 * One tree-eligible session row. Pure projection — never authoritative.
 * A thread is eligible iff workspace binding ∨ recent session ∨ needs-you.
 */
export type WorkThreadRowModel = {
  branch: string | null;
  channelId: string;
  channelName: string;
  ciGlyph: WorkThreadCiGlyph | null;
  conversationId: string;
  hasWorkspaceBinding: boolean;
  lastActivityAt: number;
  prNumber: number | null;
  prTone: WorkThreadPrTone | null;
  status: WorkThreadStatus;
  threadRootId: string;
  title: string;
  unread: boolean;
};

export type WorkTreeFolderBadge =
  | { kind: "needs-you" }
  | { kind: "live"; count: number };

export type WorkTreeFolderModel = {
  autoCollapsed: boolean;
  badge: WorkTreeFolderBadge | null;
  channelId: string;
  channelName: string;
  expanded: boolean;
  hiddenCount: number;
  lastActivityAt: number;
  pinned: boolean;
  timelineUnread: boolean;
  threads: WorkThreadRowModel[];
  visibleThreads: WorkThreadRowModel[];
};

export type WorkTreeDisclosure = {
  expanded?: boolean;
  moreExpanded?: boolean;
  pinned?: boolean;
};

export const WORK_TREE_QUIET_MS = 48 * 60 * 60 * 1_000;
export const WORK_TREE_FOLDER_CAP = 5;
export const WORK_TREE_PROMOTE_MS = 150;

export const NEEDS_YOU_KIND_ORDER = [
  "question",
  "approval",
  "evidence",
] as const;

export type NeedsYouKind = (typeof NEEDS_YOU_KIND_ORDER)[number];

export type NeedsYouItem = {
  channelId: string;
  id: string;
  kind: NeedsYouKind;
  title: string;
  threadRootId: string;
};
