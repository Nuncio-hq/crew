import type {
  ForgeCheckConclusion,
  ForgePullRequestState,
  ForgeReviewDecision,
} from "@/shared/api/threadForgeTypes";
import type { ThreadPullRequest } from "@/shared/api/thread-workspace-types";
import { parseForgePullRequestUrl } from "@/features/messages/lib/parseForgePullRequestUrl";

export const FORGE_HUB_NARROW_PX = 1100;

export const FORGE_TAB_TRIGGER_CLASS =
  "relative h-full shrink-0 rounded-none bg-transparent px-2.5 text-sm font-normal leading-5 tracking-tight text-muted-foreground shadow-none ring-offset-0 after:absolute after:inset-x-2.5 after:bottom-0 after:h-0.5 after:bg-current after:opacity-0 after:transition-opacity after:content-[''] hover:bg-transparent hover:text-foreground hover:after:opacity-40 focus-visible:ring-0 data-[state=active]:bg-transparent data-[state=active]:font-medium data-[state=active]:text-foreground data-[state=active]:shadow-none data-[state=active]:after:opacity-100";

export function forgeStateChipClass(state: ForgePullRequestState): string {
  switch (state) {
    case "open":
      return "bg-success/15 text-success";
    case "draft":
      return "bg-muted text-muted-foreground";
    case "merged":
      return "bg-merged/15 text-merged";
    case "closed":
      return "bg-destructive/15 text-destructive";
    default: {
      const _exhaustive: never = state;
      return _exhaustive;
    }
  }
}

export function forgeStateLabel(state: ForgePullRequestState): string {
  switch (state) {
    case "open":
      return "Open";
    case "draft":
      return "Draft";
    case "merged":
      return "Merged";
    case "closed":
      return "Closed";
    default: {
      const _exhaustive: never = state;
      return _exhaustive;
    }
  }
}

export function summaryStateFromThreadPullRequest(
  pullRequest: ThreadPullRequest,
): ForgePullRequestState {
  const state = pullRequest.state.toUpperCase();
  if (state === "MERGED") return "merged";
  if (state === "CLOSED") return "closed";
  if (pullRequest.isDraft) return "draft";
  return "open";
}

export function reviewDecisionLabel(decision: ForgeReviewDecision): string {
  switch (decision) {
    case "approved":
      return "Review: approved";
    case "changes-requested":
      return "Review: changes requested";
    case "review-required":
      return "Review: required";
    case "none":
      return "Review: pending";
    default: {
      const _exhaustive: never = decision;
      return _exhaustive;
    }
  }
}

export function threadReviewDecisionLabel(value: string): string {
  const upper = value.toUpperCase();
  if (upper === "APPROVED") return "Review: approved";
  if (upper === "CHANGES_REQUESTED") return "Review: changes requested";
  if (upper === "REVIEW_REQUIRED") return "Review: required";
  return "Review: pending";
}

export { summarizeCheckStates as summarizeChecks } from "../ci/checkPresentation";

export function checkConclusionIsFailed(
  conclusion: ForgeCheckConclusion,
): boolean {
  return (
    conclusion === "failure" ||
    conclusion === "cancelled" ||
    conclusion === "timed-out"
  );
}

export function refFromThreadPullRequest(
  pullRequest: ThreadPullRequest,
): { owner: string; name: string; number: number } | null {
  return parseForgePullRequestUrl(pullRequest.url);
}

export function formatIsoRelative(iso: string | null | undefined): string {
  if (!iso) return "";
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;
  const delta = Math.max(0, Date.now() - then);
  const seconds = Math.floor(delta / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function truncateMiddle(value: string, max = 48): string {
  if (value.length <= max) return value;
  const keep = Math.floor((max - 1) / 2);
  return `${value.slice(0, keep)}…${value.slice(-keep)}`;
}
