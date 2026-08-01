import type {
  ThreadPullRequest,
  ThreadPullRequestCheck,
} from "@/shared/api/thread-workspace-types";

export type ProjectThreadStatusTone =
  | "open"
  | "draft"
  | "merged"
  | "closed"
  | "success"
  | "failure"
  | "pending";

const TONE_CLASS_NAMES: Record<ProjectThreadStatusTone, string> = {
  closed: "text-destructive",
  draft: "text-muted-foreground",
  failure: "text-destructive",
  merged: "text-purple-600 dark:text-purple-400",
  open: "text-emerald-600 dark:text-emerald-400",
  pending: "text-amber-600 dark:text-amber-400",
  success: "text-emerald-600 dark:text-emerald-400",
};

export function projectThreadStatusClassName(tone: ProjectThreadStatusTone) {
  return TONE_CLASS_NAMES[tone];
}

export function pullRequestStatus(pullRequest: ThreadPullRequest) {
  const state = pullRequest.state.toUpperCase();
  if (state === "MERGED") return { label: "Merged", tone: "merged" as const };
  if (state === "CLOSED") return { label: "Closed", tone: "closed" as const };
  if (pullRequest.isDraft) return { label: "Draft", tone: "draft" as const };
  return { label: "Open", tone: "open" as const };
}

export function ciStatus(checks: readonly ThreadPullRequestCheck[]) {
  let failed = false;
  let pending = checks.length === 0;
  for (const check of checks) {
    const state = check.state.toUpperCase();
    if (["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT"].includes(state)) {
      failed = true;
    } else if (!["SUCCESS", "NEUTRAL", "SKIPPED"].includes(state)) {
      pending = true;
    }
  }
  if (failed) return { label: "Failing", tone: "failure" as const };
  if (pending) return { label: "Pending", tone: "pending" as const };
  return { label: "Passing", tone: "success" as const };
}
