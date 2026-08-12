import type { EvidenceKind } from "@/features/messages/lib/evidenceTag";
import type {
  ThreadPullRequest,
  ThreadPullRequestCheck,
} from "@/shared/api/thread-workspace-types";

/** ±10% of PR line counts (additions / deletions independently). */
export const DIFF_LINE_TOLERANCE_RATIO = 0.1;
/** Absolute file-count slack around PR `changedFiles`. */
export const DIFF_FILE_TOLERANCE = 2;

export type ParsedTestRunClaim = {
  kind: "test-run";
  passed: number;
  failed: number;
  skipped: number | null;
};

export type ParsedDiffStatClaim = {
  kind: "diff-stat";
  additions: number;
  deletions: number;
  files: number;
};

export type ParsedEvidenceClaim = ParsedTestRunClaim | ParsedDiffStatClaim;

export type EvidenceCrossCheckState =
  | "matches"
  | "diverges"
  | "ci-running"
  | "not-comparable";

export type EvidenceCrossCheckResult = {
  state: EvidenceCrossCheckState;
  label: string;
  /** Extra line under the header — only for Diverges. */
  detail: string | null;
};

const LABEL: Record<EvidenceCrossCheckState, string> = {
  matches: "Matches CI",
  diverges: "Diverges",
  "ci-running": "CI running",
  "not-comparable": "Not comparable",
};

const FAILED_CHECK_STATES = new Set([
  "FAILURE",
  "ERROR",
  "CANCELLED",
  "TIMED_OUT",
]);
const PASSED_CHECK_STATES = new Set(["SUCCESS", "NEUTRAL", "SKIPPED"]);

function notComparable(): EvidenceCrossCheckResult {
  return {
    state: "not-comparable",
    label: LABEL["not-comparable"],
    detail: null,
  };
}

function matches(): EvidenceCrossCheckResult {
  return { state: "matches", label: LABEL.matches, detail: null };
}

function ciRunning(): EvidenceCrossCheckResult {
  return { state: "ci-running", label: LABEL["ci-running"], detail: null };
}

function diverges(detail: string): EvidenceCrossCheckResult {
  return { state: "diverges", label: LABEL.diverges, detail };
}

/**
 * Parse the machine-readable claim line from an evidence body.
 * Only exact structured lines count — no substring scanning of free prose.
 * Returns null on missing/garbled input; never throws.
 */
export function parseEvidenceClaim(
  kind: EvidenceKind,
  body: string,
): ParsedEvidenceClaim | null {
  if (kind !== "test-run" && kind !== "diff-stat") return null;
  if (typeof body !== "string" || body.length === 0) return null;

  try {
    if (kind === "test-run") return parseTestRunClaim(body);
    return parseDiffStatClaim(body);
  } catch {
    return null;
  }
}

function parseTestRunClaim(body: string): ParsedTestRunClaim | null {
  for (const rawLine of body.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!/^Tests:/i.test(line)) continue;
    const rest = line.slice(line.indexOf(":") + 1).trim();
    const passed = matchCount(rest, /passed/i);
    const failed = matchCount(rest, /failed/i);
    if (passed == null || failed == null) return null;
    const skipped = matchCount(rest, /skipped/i);
    return { kind: "test-run", passed, failed, skipped };
  }
  return null;
}

function matchCount(text: string, word: RegExp): number | null {
  const match = text.match(
    new RegExp(`(\\d+)\\s*${word.source}|${word.source}\\s*(\\d+)`, "i"),
  );
  if (!match) return null;
  const value = match[1] ?? match[2];
  if (value == null) return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

function parseDiffStatClaim(body: string): ParsedDiffStatClaim | null {
  for (const rawLine of body.split(/\r?\n/)) {
    const line = rawLine.trim();
    // Diff: +120/−30 across 5 files  (ASCII or unicode minus)
    const match = line.match(
      /^Diff:\s*\+(\d+)\s*\/\s*[−-](\d+)\s+across\s+(\d+)\s+files?\s*$/i,
    );
    if (!match) continue;
    const additions = Number(match[1]);
    const deletions = Number(match[2]);
    const files = Number(match[3]);
    if (
      ![additions, deletions, files].every((n) => Number.isFinite(n) && n >= 0)
    ) {
      return null;
    }
    return { kind: "diff-stat", additions, deletions, files };
  }
  return null;
}

export function withinDiffLineTolerance(
  claimed: number,
  actual: number,
): boolean {
  const budget = Math.max(0, Math.round(actual * DIFF_LINE_TOLERANCE_RATIO));
  return Math.abs(claimed - actual) <= budget;
}

export function withinDiffFileTolerance(
  claimed: number,
  actual: number,
): boolean {
  return Math.abs(claimed - actual) <= DIFF_FILE_TOLERANCE;
}

function classifyChecks(checks: readonly ThreadPullRequestCheck[]): {
  failed: ThreadPullRequestCheck[];
  pending: boolean;
  empty: boolean;
} {
  if (checks.length === 0) {
    return { failed: [], pending: false, empty: true };
  }
  const failed: ThreadPullRequestCheck[] = [];
  let pending = false;
  for (const check of checks) {
    const state = check.state.toUpperCase();
    if (FAILED_CHECK_STATES.has(state)) failed.push(check);
    else if (!PASSED_CHECK_STATES.has(state)) pending = true;
  }
  return { failed, pending, empty: false };
}

function formatTestClaim(claim: ParsedTestRunClaim): string {
  return `${claim.passed} passed, ${claim.failed} failed`;
}

function formatFailedChecks(failed: readonly ThreadPullRequestCheck[]): string {
  if (failed.length === 0) return "all checks green";
  const names = failed
    .slice(0, 3)
    .map((check) => `${check.name} — ${check.state.toUpperCase()}`);
  const extra = failed.length > 3 ? ` (+${failed.length - 3} more)` : "";
  return names.join("; ") + extra;
}

function formatDiffClaim(claim: ParsedDiffStatClaim): string {
  return `+${claim.additions}/−${claim.deletions} across ${claim.files} files`;
}

function formatDiffPr(pr: ThreadPullRequest): string {
  return `+${pr.additions}/−${pr.deletions} across ${pr.changedFiles} files`;
}

/**
 * Compare a parsed (or parseable) evidence claim against the thread PR.
 * Missing PR, unparseable claim, or out-of-scope kinds → Not comparable.
 * Never guesses a verdict.
 */
export function compareEvidenceToPullRequest(
  kind: EvidenceKind,
  body: string,
  pullRequest: ThreadPullRequest | null | undefined,
): EvidenceCrossCheckResult {
  if (kind === "metrics" || kind === "before-after-visual") {
    return notComparable();
  }

  const claim = parseEvidenceClaim(kind, body);
  if (!claim) return notComparable();
  if (pullRequest == null) return notComparable();

  if (claim.kind === "test-run") {
    return compareTestRun(claim, pullRequest);
  }
  return compareDiffStat(claim, pullRequest);
}

function compareTestRun(
  claim: ParsedTestRunClaim,
  pullRequest: ThreadPullRequest,
): EvidenceCrossCheckResult {
  const { failed, pending, empty } = classifyChecks(pullRequest.checks);
  if (empty) return notComparable();
  if (pending) return ciRunning();

  const claimAllPass = claim.failed === 0;
  const ciAllPass = failed.length === 0;
  if (claimAllPass === ciAllPass) return matches();

  return diverges(
    `Claimed: ${formatTestClaim(claim)} · CI: ${formatFailedChecks(failed)}`,
  );
}

function compareDiffStat(
  claim: ParsedDiffStatClaim,
  pullRequest: ThreadPullRequest,
): EvidenceCrossCheckResult {
  const linesOk =
    withinDiffLineTolerance(claim.additions, pullRequest.additions) &&
    withinDiffLineTolerance(claim.deletions, pullRequest.deletions);
  const filesOk = withinDiffFileTolerance(
    claim.files,
    pullRequest.changedFiles,
  );
  if (linesOk && filesOk) return matches();

  return diverges(
    `Claimed: ${formatDiffClaim(claim)} · PR ${formatDiffPr(pullRequest)}`,
  );
}
