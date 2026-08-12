export type ThreadWorkspaceActionStatus = "completed" | "refused" | "not-found";

export type ThreadWorkspaceActionResult = {
  status: ThreadWorkspaceActionStatus;
  message: string;
};

export type ThreadWorkspaceLifecycle = {
  branchCheckedOut: boolean;
  branchExists: boolean;
  dirty: boolean | null;
  /** True when ignored/local entries exist. Presence only — never paths. */
  hasIgnoredLocalState?: boolean | null;
  worktreeExists: boolean;
};

export type ThreadPullRequestCheck = {
  name: string;
  state: string;
  url: string | null;
  workflow: string | null;
};

export type ThreadPullRequest = {
  additions: number;
  baseRefName: string;
  changedFiles: number;
  checks: ThreadPullRequestCheck[];
  closingIssuesReferences: Array<{
    number: number;
    state: string;
    title: string;
    url: string;
  }>;
  comments: Array<{
    author: { login: string } | null;
    body: string;
    createdAt: string;
    url: string;
  }>;
  deletions: number;
  headRefName: string;
  isDraft: boolean;
  mergeStateStatus: string;
  number: number;
  reviewDecision: string;
  state: string;
  title: string;
  url: string;
};

export type ThreadGitHubAvailability =
  | "available"
  | "cli-missing"
  | "cli-failed";

export type ThreadGitHubStatus = {
  availability: ThreadGitHubAvailability;
  pullRequest: ThreadPullRequest | null;
};

export type RegistryChecksState = "passing" | "failing" | "pending" | "none";

export type RegistryPullRequest = {
  number: number;
  state: string;
  isDraft: boolean;
  reviewDecision: string;
  checks: RegistryChecksState;
  additions: number;
  deletions: number;
  title: string;
  url: string;
};

/** Issue linked to a worktree via a PR closing reference. */
export type RegistryIssue = {
  number: number;
  /** Normalized `"open"` | `"closed"`. */
  state: string;
  title: string;
  url: string;
};

export type ProjectWorktreeKind = "main" | "managed" | "external";

/** Local lifecycle projection joined from durable records (Phase 3+). */
export type LifecycleIdentity = "verified" | "legacy" | "conflict";

export type ProjectWorktreeEntry = {
  worktreePath: string;
  worktreeName: string;
  branch: string | null;
  head: string;
  kind: ProjectWorktreeKind;
  rootEventId: string | null;
  prunable: boolean;
  pullRequests: RegistryPullRequest[];
  linkedIssues: RegistryIssue[];
  /** Durable routing channel when a verified lifecycle record exists. */
  routingChannelId?: string | null;
  /** Unix seconds; durable ACP creation time when known. */
  createdAt?: number | null;
  /** Unix seconds; durable ACP last-used time when known. */
  lastUsedAt?: number | null;
  /** How the registry treats local lifecycle identity. */
  lifecycleIdentity?: LifecycleIdentity | null;
};

export type GithubAvailability = "available" | "cli-missing" | "cli-failed";

export type ProjectWorktreeRegistry = {
  repositoryPath: string;
  managedRoot: string;
  github: GithubAvailability;
  entries: ProjectWorktreeEntry[];
};

export type ProjectWorktreeDetails = {
  worktreePath: string;
  dirty: boolean;
  ahead: number;
  behind: number;
  /** Unix seconds of the tip commit, when available. */
  lastCommitAt: number | null;
  diskBytes: number;
  /**
   * True when `git status --ignored` reports ignored/local entries.
   * Presence only — never includes paths or file contents.
   */
  hasIgnoredLocalState?: boolean;
};

export type CacheCategoryPreview = {
  id: string;
  label: string;
  bytes: number;
  present: boolean;
};

export type ProjectWorktreeReclaimPreview = {
  worktreePath: string;
  /** @deprecated Prefer canClearCache / canEvict — preview is not authorization. */
  actionable: boolean;
  refusalReason: string | null;
  canClearCache: boolean;
  canEvict: boolean;
  clearCacheRefusal: string | null;
  evictionRefusal: string | null;
  dirty: boolean;
  busy: boolean;
  branchRetained: boolean;
  diskBytes: number;
  cacheCategories: CacheCategoryPreview[];
  hasIgnoredLocalState: boolean;
  lifecycleIdentity: LifecycleIdentity;
  lastUsedAt: number | null;
  routingChannelId: string | null;
};

export type CacheCategoryClearResult = {
  id: string;
  status: ThreadWorkspaceActionStatus;
  message: string;
  bytesRemoved: number;
};

export type ClearProjectWorktreeCacheResult = {
  worktreePath: string;
  results: CacheCategoryClearResult[];
};

/** Alive window used by observed-time idle (#174). */
export type AliveInterval = {
  start: number;
  end: number;
};

export type WorktreeStorageAliveStatus = {
  intervals: AliveInterval[];
  recentAbsenceSecs: number;
  idleThresholdSecs: number;
  heartbeatGranuleSecs: number;
  now: number;
};

export type ReclaimTier = "lean" | "hibernate";

export type WorktreeStorageRow = {
  repositoryPath: string;
  worktreePath: string;
  worktreeName: string;
  branch: string | null;
  rootEventId: string | null;
  routingChannelId: string | null;
  lifecycleIdentity: LifecycleIdentity;
  prNumber: number | null;
  prState: string | null;
  prTitle: string | null;
  lastUsedAt: number | null;
  observedIdleSecs: number;
  wallIdleSecs: number | null;
  dirty: boolean;
  busy: boolean;
  branchPushed: boolean;
  diskBytes: number;
  cacheBytes: number;
  checkoutBytes: number;
  cacheCategoryIds: string[];
  candidate: boolean;
  tier: ReclaimTier | null;
  reason: string;
  readOnly: boolean;
  refusalReason: string | null;
  canClearCache: boolean;
  canEvict: boolean;
};

export type WorktreeStorageSnapshot = {
  rows: WorktreeStorageRow[];
  totalDiskBytes: number;
  totalCacheBytes: number;
  reclaimableBytes: number;
  candidateCount: number;
  recentAbsenceSecs: number;
  idleThresholdSecs: number;
  observedNow: number;
  intervals: AliveInterval[];
};

export type WorktreeStorageRowOutcome =
  | { status: "completed"; message: string; bytesFreed: number }
  | { status: "skipped"; message: string }
  | { status: "pending" }
  | { status: "running" };
