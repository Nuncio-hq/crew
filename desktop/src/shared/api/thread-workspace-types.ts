export type ThreadWorkspaceActionStatus = "completed" | "refused" | "not-found";

export type ThreadWorkspaceActionResult = {
  status: ThreadWorkspaceActionStatus;
  message: string;
};

export type ThreadWorkspaceLifecycle = {
  branchCheckedOut: boolean;
  branchExists: boolean;
  dirty: boolean | null;
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

export type ThreadGitHubStatus = {
  availability: "available" | "unavailable";
  pullRequest: ThreadPullRequest | null;
};
