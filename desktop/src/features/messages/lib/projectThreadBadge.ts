import type {
  ProjectWorktreeEntry,
  RegistryPullRequest,
} from "@/shared/api/thread-workspace-types";
import type { ProjectThreadStatusTone } from "@/features/messages/ui/projectThreadGitHubStatus";

export type ProjectThreadBadgePr = {
  number: number;
  tone: ProjectThreadStatusTone;
  checkGlyph: "✓" | "✕" | "⏳" | null;
  title: string;
};

export type ProjectThreadBadge = {
  label: string | null;
  branch: string;
  shortBranch: string;
  mono: boolean;
  pullRequests: ProjectThreadBadgePr[];
  overflow: number;
  diff: { additions: number; deletions: number } | null;
};

export function buildProjectThreadBadge(
  entry: ProjectWorktreeEntry,
  label: string | null,
): ProjectThreadBadge | null {
  if (!entry.branch || entry.kind !== "managed") return null;
  const shortBranch = shortBranchId(entry.branch);
  const ranked = entry.pullRequests;
  const visible = ranked.slice(0, 2);
  const overflow = Math.max(0, ranked.length - visible.length);
  const pullRequests = visible.map(toBadgePr);
  const diff = sumDiff(ranked);
  return {
    label,
    branch: entry.branch,
    shortBranch,
    mono: label == null,
    pullRequests,
    overflow,
    diff,
  };
}

function shortBranchId(branch: string): string {
  const short = branch.replace(/^buzz\//, "");
  return short.length > 8 ? short.slice(0, 8) : short;
}

function toBadgePr(pr: RegistryPullRequest): ProjectThreadBadgePr {
  return {
    number: pr.number,
    tone: prTone(pr),
    checkGlyph: checkGlyph(pr),
    title: pr.title,
  };
}

function prTone(pr: RegistryPullRequest): ProjectThreadStatusTone {
  const state = pr.state.toUpperCase();
  if (state === "MERGED") return "merged";
  if (state === "CLOSED") return "closed";
  if (pr.isDraft) return "draft";
  if (pr.checks === "failing") return "failure";
  if (pr.checks === "passing") return "success";
  if (pr.checks === "pending") return "pending";
  return "open";
}

function checkGlyph(pr: RegistryPullRequest): "✓" | "✕" | "⏳" | null {
  const state = pr.state.toUpperCase();
  if (state === "MERGED" || state === "CLOSED" || pr.isDraft) return null;
  if (pr.checks === "passing") return "✓";
  if (pr.checks === "failing") return "✕";
  if (pr.checks === "pending") return "⏳";
  return null;
}

function sumDiff(
  prs: readonly RegistryPullRequest[],
): { additions: number; deletions: number } | null {
  if (prs.length === 0) return null;
  // Prefer the highest-ranked (first) open/draft PR's diff; else first PR.
  const primary = prs[0];
  if (!primary) return null;
  if (primary.additions === 0 && primary.deletions === 0) return null;
  return { additions: primary.additions, deletions: primary.deletions };
}
