import type {
  GithubAvailability,
  ProjectWorktreeDetails,
  ProjectWorktreeEntry,
  RegistryPullRequest,
} from "@/shared/api/thread-workspace-types";

export const IDLE_QUIET_DAYS = 7;
export const IDLE_QUIET_MS = IDLE_QUIET_DAYS * 24 * 60 * 60 * 1000;

export type WorktreeBucketId =
  | "active"
  | "ready-to-merge"
  | "idle"
  | "orphan"
  | "broken"
  | "other-channel"
  | "channel-unknown"
  | "conflict"
  | "other";

export type WorktreeBucketItem = {
  entry: ProjectWorktreeEntry;
  bucket: WorktreeBucketId;
  /**
   * Legacy no-root orphan, or channel identity not yet durable / not this
   * channel. Never sufficient for destructive authorization.
   */
  orphanReason?: "unknown" | "channel-unknown";
};

export type WorktreeBucket = {
  id: WorktreeBucketId;
  label: string;
  hint: string;
  items: WorktreeBucketItem[];
  readonly: boolean;
};

export type BucketWorktreesInput = {
  entries: readonly ProjectWorktreeEntry[];
  /**
   * Thread root event ids currently visible for this channel.
   * Used only as a Phase-1 fallback when durable routing is absent.
   * Never proves another-channel ownership by itself.
   */
  channelRootIds: ReadonlySet<string>;
  /** Conversation/thread roots with a live agent turn (presentation only). */
  activeRootIds: ReadonlySet<string>;
  /** Current channel id for durable routing matches (Phase 3+). */
  channelId?: string | null;
  detailsByPath?: ReadonlyMap<string, ProjectWorktreeDetails>;
  nowMs?: number;
};

/** Devin-style channel rollup across managed worktree entries. */
export type GithubRollupCounts = {
  prOpen: number;
  prDraft: number;
  prMerged: number;
  prClosed: number;
  issuesOpen: number;
  issuesClosed: number;
};

const BUCKET_META: Record<
  WorktreeBucketId,
  { label: string; hint: string; readonly: boolean }
> = {
  active: {
    label: "Active",
    hint: "agent running, or PR open",
    readonly: false,
  },
  "ready-to-merge": {
    label: "Ready to merge",
    hint: "approved + checks green",
    readonly: false,
  },
  idle: {
    label: "Idle",
    hint: "clean · no open PR · quiet 7 days",
    readonly: false,
  },
  orphan: {
    label: "Orphan",
    hint: "no thread-root record — read-only until adopted",
    readonly: true,
  },
  broken: {
    label: "Broken",
    hint: "directory gone — prune",
    readonly: false,
  },
  "other-channel": {
    label: "Other channels",
    hint: "read-only — owned by another channel",
    readonly: true,
  },
  "channel-unknown": {
    label: "Legacy / channel unknown",
    hint: "read-only — durable channel identity missing",
    readonly: true,
  },
  conflict: {
    label: "Needs attention",
    hint: "read-only — lifecycle record conflict",
    readonly: true,
  },
  other: {
    label: "Other checkouts",
    hint: "read-only — never touched by bulk actions",
    readonly: true,
  },
};

const ORDER: WorktreeBucketId[] = [
  "active",
  "ready-to-merge",
  "idle",
  "orphan",
  "broken",
  "other-channel",
  "channel-unknown",
  "conflict",
  "other",
];

export function bucketWorktrees(input: BucketWorktreesInput): WorktreeBucket[] {
  const now = input.nowMs ?? Date.now();
  const grouped = new Map<WorktreeBucketId, WorktreeBucketItem[]>();
  for (const id of ORDER) grouped.set(id, []);

  for (const entry of input.entries) {
    const item = classifyEntry(entry, input, now);
    grouped.get(item.bucket)?.push(item);
  }

  return ORDER.map((id) => {
    const meta = BUCKET_META[id];
    return {
      id,
      label: meta.label,
      hint: meta.hint,
      readonly: meta.readonly,
      items: grouped.get(id) ?? [],
    };
  }).filter((bucket) => bucket.items.length > 0);
}

/**
 * Central actionability helper so row and bulk selection cannot drift.
 * Frontend checks are presentation only — Rust revalidates before mutation.
 */
export function canReclaimWorktree(
  item: WorktreeBucketItem,
  options?: { activeRootIds?: ReadonlySet<string> },
): boolean {
  if (!isChannelScopedManaged(item)) return false;
  if (
    item.bucket === "broken" ||
    item.bucket === "active" ||
    item.bucket === "ready-to-merge"
  ) {
    return false;
  }
  if (item.bucket !== "idle") return false;

  const root = item.entry.rootEventId?.toLowerCase() ?? null;
  if (root && options?.activeRootIds) {
    const active = [...options.activeRootIds].some(
      (id) => id.toLowerCase() === root,
    );
    if (active) return false;
  }
  return true;
}

/**
 * Cache clear is allowed on dirty verified/same-channel rows; active turns and
 * unknown/other-channel identities remain read-only. Backend still authorizes.
 */
export function canClearCacheWorktree(
  item: WorktreeBucketItem,
  options?: { activeRootIds?: ReadonlySet<string> },
): boolean {
  if (!isChannelScopedManaged(item)) return false;
  if (item.bucket === "broken") return false;
  const root = item.entry.rootEventId?.toLowerCase() ?? null;
  if (root && options?.activeRootIds) {
    const active = [...options.activeRootIds].some(
      (id) => id.toLowerCase() === root,
    );
    if (active) return false;
  }
  // Ready-to-merge / active-with-PR can still clear cache (no exclusive agent
  // lease from presentation alone); backend refuses if busy.
  return (
    item.bucket === "idle" ||
    item.bucket === "active" ||
    item.bucket === "ready-to-merge"
  );
}

function isChannelScopedManaged(item: WorktreeBucketItem): boolean {
  if (item.entry.kind === "main" || item.entry.kind === "external") {
    return false;
  }
  if (item.entry.prunable) return false;
  if (
    item.bucket === "other" ||
    item.bucket === "other-channel" ||
    item.bucket === "channel-unknown" ||
    item.bucket === "conflict" ||
    item.bucket === "orphan"
  ) {
    return false;
  }
  if (
    item.orphanReason === "unknown" ||
    item.orphanReason === "channel-unknown"
  ) {
    return false;
  }
  const identity = item.entry.lifecycleIdentity ?? null;
  if (identity === "legacy" || identity === "conflict") {
    return false;
  }
  if (identity != null && identity !== "verified") {
    return false;
  }
  return true;
}

/** Paths that remain selectable after a registry/bucket refresh. */
export function pruneSelectedWorktreePaths(
  selected: ReadonlySet<string>,
  buckets: readonly WorktreeBucket[],
  options?: { activeRootIds?: ReadonlySet<string> },
): Set<string> {
  const actionable = new Set<string>();
  for (const bucket of buckets) {
    for (const item of bucket.items) {
      if (canReclaimWorktree(item, options)) {
        actionable.add(item.entry.worktreePath);
      }
    }
  }
  const next = new Set<string>();
  for (const path of selected) {
    if (actionable.has(path)) next.add(path);
  }
  return next;
}

function classifyEntry(
  entry: ProjectWorktreeEntry,
  input: BucketWorktreesInput,
  nowMs: number,
): WorktreeBucketItem {
  if (entry.kind === "main" || entry.kind === "external") {
    return { entry, bucket: "other" };
  }
  if (entry.prunable) {
    return { entry, bucket: "broken" };
  }

  const identity = entry.lifecycleIdentity ?? null;
  if (identity === "conflict") {
    return { entry, bucket: "conflict" };
  }

  const root = entry.rootEventId?.toLowerCase() ?? null;
  if (!root) {
    return { entry, bucket: "orphan", orphanReason: "unknown" };
  }

  // Durable routing (Phase 3): prefer verified channel identity over the
  // paginated timeline window.
  if (identity === "verified" && entry.routingChannelId && input.channelId) {
    if (entry.routingChannelId !== input.channelId) {
      return { entry, bucket: "other-channel" };
    }
  } else if (identity === "legacy") {
    return {
      entry,
      bucket: "channel-unknown",
      orphanReason: "channel-unknown",
    };
  } else {
    // No durable identity yet (Phase 1): absence from the loaded timeline
    // never proves other-channel ownership and is not actionable.
    const inVisibleChannel = [...input.channelRootIds].some(
      (id) => id.toLowerCase() === root,
    );
    if (!inVisibleChannel) {
      return {
        entry,
        bucket: "channel-unknown",
        orphanReason: "channel-unknown",
      };
    }
  }

  const openPrs = openPullRequests(entry.pullRequests);
  if (isReadyToMerge(openPrs)) {
    return { entry, bucket: "ready-to-merge" };
  }

  const agentActive = [...input.activeRootIds].some(
    (id) => id.toLowerCase() === root,
  );
  if (agentActive || openPrs.length > 0) {
    return { entry, bucket: "active" };
  }

  if (isIdleEntry(entry, input.detailsByPath?.get(entry.worktreePath), nowMs)) {
    return { entry, bucket: "idle" };
  }

  // Recent activity, dirty, or details not loaded yet — keep visible as Active.
  return { entry, bucket: "active" };
}

function openPullRequests(
  pullRequests: readonly RegistryPullRequest[],
): RegistryPullRequest[] {
  return pullRequests.filter((pr) => {
    const state = pr.state.toUpperCase();
    return state === "OPEN" || pr.isDraft;
  });
}

function isReadyToMerge(openPrs: readonly RegistryPullRequest[]): boolean {
  return openPrs.some(
    (pr) =>
      !pr.isDraft &&
      pr.state.toUpperCase() === "OPEN" &&
      pr.reviewDecision.toUpperCase() === "APPROVED" &&
      pr.checks === "passing",
  );
}

/**
 * Prefer durable ACP `lastUsedAt` when present (Phase 3+).
 * Missing `lastUsedAt` never means idle once a verified identity exists;
 * Phase 1 may still fall back to commit age for pre-metadata rows.
 */
function isIdleEntry(
  entry: ProjectWorktreeEntry,
  details: ProjectWorktreeDetails | undefined,
  nowMs: number,
): boolean {
  if (details?.dirty) return false;
  // Ignored/local state is never "safely idle" for eviction presentation.
  if (details?.hasIgnoredLocalState) return false;
  if (entry.lastUsedAt != null) {
    return nowMs - entry.lastUsedAt * 1000 >= IDLE_QUIET_MS;
  }
  if (entry.lifecycleIdentity === "verified") {
    // Verified rows without lastUsedAt are never idle.
    return false;
  }
  if (details?.lastCommitAt == null) return false;
  return nowMs - details.lastCommitAt * 1000 >= IDLE_QUIET_MS;
}

export function countManagedWorktrees(
  entries: readonly ProjectWorktreeEntry[],
): number {
  return entries.filter((entry) => entry.kind === "managed").length;
}

export function countOpenPullRequests(
  entries: readonly ProjectWorktreeEntry[],
): number {
  const seen = new Set<number>();
  for (const entry of entries) {
    if (entry.kind !== "managed") continue;
    for (const pr of openPullRequests(entry.pullRequests)) {
      seen.add(pr.number);
    }
  }
  return seen.size;
}

/**
 * Aggregate PR/issue state counts across managed registry entries.
 * Dedupes by PR/issue number so the same item on two worktrees counts once.
 */
export function aggregateGithubRollup(
  entries: readonly ProjectWorktreeEntry[],
): GithubRollupCounts {
  const prSeen = new Set<number>();
  const issueSeen = new Set<number>();
  const counts: GithubRollupCounts = {
    prOpen: 0,
    prDraft: 0,
    prMerged: 0,
    prClosed: 0,
    issuesOpen: 0,
    issuesClosed: 0,
  };

  for (const entry of entries) {
    if (entry.kind !== "managed") continue;
    for (const pr of entry.pullRequests) {
      if (prSeen.has(pr.number)) continue;
      prSeen.add(pr.number);
      const state = pr.state.toUpperCase();
      if (state === "MERGED") {
        counts.prMerged += 1;
      } else if (state === "CLOSED") {
        counts.prClosed += 1;
      } else if (pr.isDraft) {
        counts.prDraft += 1;
      } else {
        counts.prOpen += 1;
      }
    }
    for (const issue of entry.linkedIssues ?? []) {
      if (issueSeen.has(issue.number)) continue;
      issueSeen.add(issue.number);
      if (issue.state.toLowerCase() === "open") {
        counts.issuesOpen += 1;
      } else {
        counts.issuesClosed += 1;
      }
    }
  }
  return counts;
}

/** Channel-header pill label; degraded GitHub stays distinguishable from 0 PRs. */
export function channelWorktreesPillLabel(
  managed: number,
  openPrs: number,
  github: GithubAvailability,
): string {
  if (github !== "available") {
    return `${managed} worktrees · PRs unavailable`;
  }
  if (openPrs > 0) {
    return `${managed} worktrees · ${openPrs} PR${openPrs === 1 ? "" : "s"} open`;
  }
  return `${managed} worktrees`;
}

export function githubAvailabilityNotice(
  github: GithubAvailability,
): string | null {
  if (github === "cli-missing") {
    return "GitHub CLI (gh) not found — PR and issue status unavailable.";
  }
  if (github === "cli-failed") {
    return "GitHub CLI could not read this repo — PR and issue status unavailable.";
  }
  return null;
}

export { formatDiskBytes } from "./worktreeDiskFormat";
