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
  | "other";

export type WorktreeBucketItem = {
  entry: ProjectWorktreeEntry;
  bucket: WorktreeBucketId;
  /** Root exists but belongs to another channel. */
  orphanReason?: "unknown" | "other-channel";
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
  /** Thread root event ids that belong to this channel. */
  channelRootIds: ReadonlySet<string>;
  /** Conversation/thread roots with a live agent turn. */
  activeRootIds: ReadonlySet<string>;
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
    hint: "no thread-root record",
    readonly: false,
  },
  broken: {
    label: "Broken",
    hint: "directory gone — prune",
    readonly: false,
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

  const root = entry.rootEventId?.toLowerCase() ?? null;
  if (!root) {
    return { entry, bucket: "orphan", orphanReason: "unknown" };
  }
  if (![...input.channelRootIds].some((id) => id.toLowerCase() === root)) {
    return { entry, bucket: "orphan", orphanReason: "other-channel" };
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

  const details = input.detailsByPath?.get(entry.worktreePath);
  if (details && isIdle(details, nowMs)) {
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

function isIdle(details: ProjectWorktreeDetails, nowMs: number): boolean {
  if (details.dirty) return false;
  if (details.lastCommitAt == null) return false;
  const ageMs = nowMs - details.lastCommitAt * 1000;
  return ageMs >= IDLE_QUIET_MS;
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
