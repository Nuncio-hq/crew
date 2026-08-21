import type { ThreadPullRequestCheck } from "@/shared/api/thread-workspace-types";

const FAILED_STATES = new Set(["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT"]);
const PASSED_STATES = new Set(["SUCCESS", "NEUTRAL", "SKIPPED"]);

export type CheckBucket = "passed" | "failed" | "running";

export type CheckTone = "success" | "destructive" | "attention" | "muted";

/** Classify a forge/GitHub check state into a display bucket. */
export function checkBucket(state: string): CheckBucket {
  const upper = state.toUpperCase().replace(/-/g, "_");
  if (PASSED_STATES.has(upper)) return "passed";
  if (FAILED_STATES.has(upper)) return "failed";
  return "running";
}

export function checkTone(state: string): CheckTone {
  const bucket = checkBucket(state);
  switch (bucket) {
    case "passed":
      return "success";
    case "failed":
      return "destructive";
    case "running":
      return "attention";
    default: {
      const _exhaustive: never = bucket;
      return _exhaustive;
    }
  }
}

/** Short human label for a check conclusion/state. */
export function checkStateLabel(state: string): string {
  switch (state.toUpperCase().replace(/-/g, "_")) {
    case "SUCCESS":
      return "Passed";
    case "NEUTRAL":
      return "Neutral";
    case "SKIPPED":
      return "Skipped";
    case "FAILURE":
      return "Failed";
    case "ERROR":
      return "Error";
    case "CANCELLED":
      return "Cancelled";
    case "TIMED_OUT":
      return "Timed out";
    case "PENDING":
    case "QUEUED":
    case "IN_PROGRESS":
    case "WAITING":
    case "REQUESTED":
      return "Running";
    case "UNKNOWN":
      return "Unknown";
    default:
      return state.trim() || "Unknown";
  }
}

export function summarizeCheckStates(states: readonly string[]): {
  passed: number;
  failed: number;
  running: number;
} {
  let passed = 0;
  let failed = 0;
  for (const value of states) {
    const bucket = checkBucket(value);
    if (bucket === "passed") passed += 1;
    else if (bucket === "failed") failed += 1;
  }
  return {
    failed,
    passed,
    running: Math.max(0, states.length - passed - failed),
  };
}

export function summarizeThreadChecks(
  checks: readonly ThreadPullRequestCheck[],
) {
  return summarizeCheckStates(checks.map((check) => check.state));
}

export const CHECK_TONE_TEXT: Record<CheckTone, string> = {
  success: "text-success",
  destructive: "text-destructive",
  attention: "text-attention",
  muted: "text-muted-foreground",
};

export const CHECK_TONE_DOT: Record<CheckTone, string> = {
  success: "bg-success",
  destructive: "bg-destructive",
  attention: "bg-attention",
  muted: "bg-muted-foreground",
};
