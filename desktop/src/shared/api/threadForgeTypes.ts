export type ForgeAvailability =
  | "available"
  | "cli-missing"
  | "cli-failed"
  | "rate-limited";

export type ForgePullRequestState = "open" | "draft" | "merged" | "closed";

export type ForgeReviewDecision =
  | "none"
  | "review-required"
  | "approved"
  | "changes-requested";

export type ForgeFileViewedState = "viewed" | "unviewed" | "dismissed";

export type ForgeCheckConclusion =
  | "success"
  | "failure"
  | "neutral"
  | "cancelled"
  | "skipped"
  | "timed-out"
  | "action-required"
  | "pending"
  | "unknown";

export type ForgeMergeStrategy = "merge" | "squash" | "rebase";

export type ForgeReviewEvent = "approve" | "request-changes" | "comment";

export type ForgeDiffSource = "worktree" | "api";

export type ForgePullRequestRef = {
  owner: string;
  name: string;
  number: number;
};

export type ForgeAuthor = {
  login: string;
};

export type ForgeComment = {
  id: string;
  author: ForgeAuthor | null;
  body: string;
  createdAt: string;
  url: string;
};

export type ForgeReview = {
  id: string;
  author: ForgeAuthor | null;
  body: string;
  state: string;
  submittedAt: string | null;
  url: string;
};

export type ForgeReviewThread = {
  id: string;
  isResolved: boolean;
  isOutdated: boolean;
  path: string | null;
  line: number | null;
  comments: ForgeComment[];
};

export type ForgeCommit = {
  oid: string;
  messageHeadline: string;
  committedAt: string;
  additions: number;
  deletions: number;
  authorName: string | null;
  authorEmail: string | null;
};

export type ForgeChangedFile = {
  path: string;
  additions: number;
  deletions: number;
  viewedState: ForgeFileViewedState;
};

export type ForgeCheck = {
  name: string;
  status: string;
  conclusion: ForgeCheckConclusion;
  url: string | null;
  workflow: string | null;
  runId: number | null;
  startedAt: string | null;
  completedAt: string | null;
};

export type ForgePullRequestDetail = {
  id: string;
  number: number;
  title: string;
  body: string;
  url: string;
  state: ForgePullRequestState;
  isDraft: boolean;
  headRefName: string;
  baseRefName: string;
  additions: number;
  deletions: number;
  changedFiles: number;
  reviewDecision: ForgeReviewDecision;
  mergeStateStatus: string;
  author: ForgeAuthor | null;
  comments: ForgeComment[];
  reviews: ForgeReview[];
  reviewThreads: ForgeReviewThread[];
  commits: ForgeCommit[];
  files: ForgeChangedFile[];
  checks: ForgeCheck[];
  mergeStrategies: ForgeMergeStrategy[];
  filesTruncated: boolean;
  commitsTruncated: boolean;
  checksTruncated: boolean;
};

export type ForgeDetailResult = {
  availability: ForgeAvailability;
  rateLimitedUntil: string | null;
  detail: ForgePullRequestDetail | null;
  message: string | null;
};

export type ForgeDiffFile = {
  path: string;
  additions: number;
  deletions: number;
  patch: string;
  truncated: boolean;
};

export type ForgeDiff = {
  files: ForgeDiffFile[];
  additions: number;
  deletions: number;
  source: ForgeDiffSource;
};

export type ForgeDiffResult = {
  availability: ForgeAvailability;
  rateLimitedUntil: string | null;
  diff: ForgeDiff | null;
  message: string | null;
};

export type ForgeCheckLogTail = {
  job: string;
  step: string;
  lines: string[];
  truncated: boolean;
};

export type ForgeCheckLogResult = {
  availability: ForgeAvailability;
  rateLimitedUntil: string | null;
  tails: ForgeCheckLogTail[];
  message: string | null;
};

export type ForgeActionResult = {
  ok: boolean;
  message: string;
};
