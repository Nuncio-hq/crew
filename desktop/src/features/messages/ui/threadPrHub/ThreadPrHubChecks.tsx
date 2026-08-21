import { ExternalLink, LoaderCircle } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import {
  getForgeCheckLogTail,
  rerunForgeChecks,
} from "@/shared/api/threadForge";
import { invalidateThreadForgePullRequestStore } from "@/features/messages/lib/threadForgePullRequestStore";
import type { ThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import type {
  ForgeCheck,
  ForgeCheckConclusion,
  ForgeCheckLogTail,
} from "@/shared/api/threadForgeTypes";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import { CheckStatusDot } from "../ci/CiPresentation";
import { checkConclusionIsFailed } from "./forgeHubCopy";

type CheckGroupId = "failed" | "running" | "pending" | "passed" | "skipped";

const GROUP_ORDER: CheckGroupId[] = [
  "failed",
  "running",
  "pending",
  "passed",
  "skipped",
];

const GROUP_LABEL: Record<CheckGroupId, string> = {
  failed: "Failed",
  running: "Running",
  pending: "Pending",
  passed: "Passed",
  skipped: "Skipped",
};

/**
 * Cursor-style Checks tab: group by status, compact rows (icon · name · time).
 */
export function ThreadPrHubChecks({
  checks,
  onRefresh,
  refIdentity,
}: {
  checks: ForgeCheck[];
  onRefresh: () => void;
  refIdentity: Extract<ThreadForgeHubSubject, { kind: "pr" }>;
}) {
  const groups = React.useMemo(() => groupByStatus(checks), [checks]);
  const [expanded, setExpanded] = React.useState<string | null>(null);
  const [tails, setTails] = React.useState<ForgeCheckLogTail[] | null>(null);
  const [busyRun, setBusyRun] = React.useState<number | null>(null);
  const [collapsed, setCollapsed] = React.useState<Set<CheckGroupId>>(
    () => new Set(["passed", "skipped"]),
  );

  const failed = checks.filter((check) =>
    checkConclusionIsFailed(check.conclusion),
  );

  async function loadTail(check: ForgeCheck) {
    const key = checkKey(check);
    if (expanded === key) {
      setExpanded(null);
      return;
    }
    setExpanded(key);
    if (!check.runId) {
      setTails([]);
      return;
    }
    setTails(null);
    const result = await getForgeCheckLogTail({
      owner: refIdentity.owner,
      name: refIdentity.name,
      number: refIdentity.number,
      runId: check.runId,
    });
    setTails(result.tails);
  }

  async function rerun(runId: number, failedOnly: boolean) {
    setBusyRun(runId);
    try {
      await rerunForgeChecks({
        owner: refIdentity.owner,
        name: refIdentity.name,
        number: refIdentity.number,
        runId,
        failedOnly,
      });
      invalidateThreadForgePullRequestStore();
      onRefresh();
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Could not re-run checks.",
      );
    } finally {
      setBusyRun(null);
    }
  }

  const failedRunIds = uniqueRunIds(failed);

  function toggleGroup(id: CheckGroupId) {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden"
      data-testid="thread-pr-hub-checks"
    >
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 px-3 py-1.5">
        <p className="text-2xs text-muted-foreground">
          {failed.length > 0
            ? `${failed.length} failed`
            : `${checks.length} checks`}
        </p>
        {failedRunIds.length > 0 ? (
          <Button
            data-testid="thread-pr-hub-rerun-failed"
            disabled={busyRun !== null}
            onClick={() => {
              for (const runId of failedRunIds) {
                void rerun(runId, true);
              }
            }}
            size="xs"
            type="button"
            variant="outline"
          >
            Re-run failed
          </Button>
        ) : null}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        {groups.map(([groupId, rows]) => {
          const isCollapsed = collapsed.has(groupId);
          return (
            <section className="mb-2" key={groupId}>
              <button
                className="flex w-full items-center gap-1.5 px-1 py-1 text-left text-2xs font-medium text-muted-foreground hover:text-foreground"
                onClick={() => toggleGroup(groupId)}
                type="button"
              >
                <span aria-hidden="true" className="w-3 tabular-nums">
                  {isCollapsed ? "▸" : "▾"}
                </span>
                {GROUP_LABEL[groupId]}
                <span className="tabular-nums text-muted-foreground/70">
                  ({rows.length})
                </span>
              </button>
              {isCollapsed
                ? null
                : rows.map((check) => {
                    const key = checkKey(check);
                    const open = expanded === key;
                    const failedRow = checkConclusionIsFailed(check.conclusion);
                    const duration = formatCheckDuration(check);
                    return (
                      <div className="mb-0.5" key={key}>
                        <div className="flex items-center gap-2 rounded-md px-1.5 py-1 hover:bg-muted/40">
                          <button
                            className="flex min-w-0 flex-1 items-center gap-2 text-left"
                            data-testid={`thread-pr-hub-check-${check.name}`}
                            onClick={() => void loadTail(check)}
                            type="button"
                          >
                            <CheckStatusDot state={check.conclusion} />
                            <span className="min-w-0 flex-1 truncate text-sm">
                              {displayCheckName(check)}
                            </span>
                            {duration ? (
                              <span className="shrink-0 tabular-nums text-2xs text-muted-foreground">
                                {duration}
                              </span>
                            ) : null}
                          </button>
                          {check.url ? (
                            <a
                              className="shrink-0 text-muted-foreground hover:text-foreground"
                              href={check.url}
                              rel="noreferrer"
                              target="_blank"
                              title="Open on GitHub"
                            >
                              <ExternalLink className="h-3.5 w-3.5" />
                            </a>
                          ) : null}
                          {check.runId && failedRow ? (
                            <Button
                              disabled={busyRun === check.runId}
                              onClick={() =>
                                void rerun(check.runId as number, false)
                              }
                              size="xs"
                              type="button"
                              variant="ghost"
                            >
                              Re-run
                            </Button>
                          ) : null}
                        </div>
                        {open ? (
                          <div
                            className={cn(
                              "ml-4 border-l border-border/60 bg-muted/20 p-2 font-mono text-2xs",
                              failedRow
                                ? "text-destructive"
                                : "text-muted-foreground",
                            )}
                            data-testid="thread-pr-hub-check-log"
                          >
                            {tails === null ? (
                              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                            ) : tails.length === 0 ? (
                              <p>No log tail.</p>
                            ) : (
                              tails.map((tail) => (
                                <div
                                  className="mb-2"
                                  key={`${tail.job}:${tail.step}`}
                                >
                                  <div className="font-semibold text-foreground">
                                    {tail.job} / {tail.step}
                                    {tail.truncated ? " (tail)" : ""}
                                  </div>
                                  <pre className="max-h-64 overflow-auto whitespace-pre-wrap">
                                    {tail.lines.join("\n")}
                                  </pre>
                                </div>
                              ))
                            )}
                          </div>
                        ) : null}
                      </div>
                    );
                  })}
            </section>
          );
        })}
        {checks.length === 0 ? (
          <p className="px-1 text-sm text-muted-foreground">No checks yet.</p>
        ) : null}
      </div>
    </div>
  );
}

function statusGroup(check: ForgeCheck): CheckGroupId {
  const conclusion = check.conclusion;
  if (checkConclusionIsFailed(conclusion)) return "failed";
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

function groupByStatus(
  checks: ForgeCheck[],
): Array<[CheckGroupId, ForgeCheck[]]> {
  const map = new Map<CheckGroupId, ForgeCheck[]>();
  for (const id of GROUP_ORDER) map.set(id, []);
  for (const check of checks) {
    const id = statusGroup(check);
    map.get(id)?.push(check);
  }
  return GROUP_ORDER.filter((id) => (map.get(id)?.length ?? 0) > 0).map(
    (id) => [id, map.get(id) ?? []],
  );
}

/** Prefer short name; drop redundant "Workflow / " when name already includes it. */
function displayCheckName(check: ForgeCheck): string {
  const name = check.name.trim();
  const workflow = check.workflow?.trim();
  if (!workflow) return name;
  const prefix = `${workflow} / `;
  if (name.startsWith(prefix)) return name.slice(prefix.length);
  return name;
}

function formatCheckDuration(check: ForgeCheck): string {
  if (!check.startedAt) return "";
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

function uniqueRunIds(checks: ForgeCheck[]): number[] {
  return [
    ...new Set(
      checks
        .map((check) => check.runId)
        .filter((id): id is number => typeof id === "number"),
    ),
  ];
}

function checkKey(check: ForgeCheck): string {
  return `${check.workflow ?? ""}:${check.name}:${check.runId ?? 0}`;
}
