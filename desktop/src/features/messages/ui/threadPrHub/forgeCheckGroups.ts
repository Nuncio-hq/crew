import type {
  ForgeCheck,
  ForgeCheckConclusion,
} from "@/shared/api/threadForgeTypes";

export type CheckGroupId =
  | "failed"
  | "running"
  | "pending"
  | "passed"
  | "skipped";

export const CHECK_GROUP_ORDER: CheckGroupId[] = [
  "failed",
  "running",
  "pending",
  "passed",
  "skipped",
];

export const CHECK_GROUP_LABEL: Record<CheckGroupId, string> = {
  failed: "Failed",
  running: "Running",
  pending: "Pending",
  passed: "Passed",
  skipped: "Skipped",
};

function isFailedConclusion(conclusion: ForgeCheckConclusion): boolean {
  return (
    conclusion === "failure" ||
    conclusion === "cancelled" ||
    conclusion === "timed-out"
  );
}

export function forgeCheckGroup(check: ForgeCheck): CheckGroupId {
  const conclusion = check.conclusion;
  if (isFailedConclusion(conclusion)) return "failed";
  if (conclusion === "skipped") return "skipped";
  if (conclusion === "success" || conclusion === "neutral") return "passed";
  if (isRunningConclusion(conclusion, check.status)) return "running";
  return "pending";
}

function isRunningConclusion(
  conclusion: ForgeCheckConclusion,
  status: string,
): boolean {
  if (conclusion === "pending" || conclusion === "action-required") return true;
  const upper = status.toUpperCase();
  return (
    upper === "IN_PROGRESS" ||
    upper === "IN PROGRESS" ||
    upper.includes("PROGRESS")
  );
}

export function groupForgeChecksByStatus(
  checks: readonly ForgeCheck[],
): Array<[CheckGroupId, ForgeCheck[]]> {
  const map = new Map<CheckGroupId, ForgeCheck[]>();
  for (const id of CHECK_GROUP_ORDER) map.set(id, []);
  for (const check of checks) {
    map.get(forgeCheckGroup(check))?.push(check);
  }
  return CHECK_GROUP_ORDER.filter((id) => (map.get(id)?.length ?? 0) > 0).map(
    (id) => [id, map.get(id) ?? []],
  );
}

/**
 * Cursor Checks tab chip: "8/14 Running" | "3 Failed" | "12 Passed".
 * completed = terminal (passed + failed + skipped).
 */
export function summarizeChecksTab(checks: readonly ForgeCheck[]): {
  kind: "running" | "failed" | "passed" | "empty";
  /** Primary label after "Checks ", e.g. "8/14 Running". */
  label: string;
  completed: number;
  total: number;
  failed: number;
  running: number;
} {
  const total = checks.length;
  if (total === 0) {
    return {
      kind: "empty",
      label: "—",
      completed: 0,
      total: 0,
      failed: 0,
      running: 0,
    };
  }
  let failed = 0;
  let running = 0;
  let pending = 0;
  let passed = 0;
  let skipped = 0;
  for (const check of checks) {
    const group = forgeCheckGroup(check);
    switch (group) {
      case "failed":
        failed += 1;
        break;
      case "running":
        running += 1;
        break;
      case "pending":
        pending += 1;
        break;
      case "passed":
        passed += 1;
        break;
      case "skipped":
        skipped += 1;
        break;
      default: {
        const _exhaustive: never = group;
        return _exhaustive;
      }
    }
  }
  const inFlight = running + pending;
  const completed = passed + failed + skipped;
  if (inFlight > 0) {
    return {
      kind: "running",
      label: `${completed}/${total} Running`,
      completed,
      total,
      failed,
      running: inFlight,
    };
  }
  if (failed > 0) {
    return {
      kind: "failed",
      label: `${failed} Failed`,
      completed,
      total,
      failed,
      running: 0,
    };
  }
  return {
    kind: "passed",
    label: `${passed} Passed`,
    completed,
    total,
    failed: 0,
    running: 0,
  };
}

/** Cursor keeps "Workflow / Job" on each row. */
export function displayForgeCheckName(check: ForgeCheck): string {
  const name = check.name.trim();
  const workflow = check.workflow?.trim();
  if (!workflow) return name;
  if (
    name.startsWith(`${workflow} / `) ||
    name.startsWith(`${workflow}/`) ||
    name.includes(` / `)
  ) {
    return name;
  }
  return `${workflow} / ${name}`;
}

export function formatForgeCheckDuration(check: ForgeCheck): string {
  if (!check.startedAt) return check.conclusion === "skipped" ? "0s" : "";
  const start = Date.parse(check.startedAt);
  if (Number.isNaN(start)) return "";
  const end = check.completedAt ? Date.parse(check.completedAt) : Date.now();
  if (Number.isNaN(end) || end < start) return "";
  const seconds = Math.floor((end - start) / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rem = seconds % 60;
  if (minutes < 60) return rem === 0 ? `${minutes}m` : `${minutes}m ${rem}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}
