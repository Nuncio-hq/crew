import {
  Check,
  Circle,
  CircleMinus,
  ExternalLink,
  LoaderCircle,
  X,
} from "lucide-react";
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
  ForgeCheckLogTail,
} from "@/shared/api/threadForgeTypes";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import { checkConclusionIsFailed } from "./forgeHubCopy";
import {
  CHECK_GROUP_LABEL,
  type CheckGroupId,
  displayForgeCheckName,
  formatForgeCheckDuration,
  groupForgeChecksByStatus,
} from "./forgeCheckGroups";

/**
 * Cursor-style Checks list: status groups, circle icons, name + duration.
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
  const groups = React.useMemo(
    () => groupForgeChecksByStatus(checks),
    [checks],
  );
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
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/50 px-3 py-1.5">
        <p className="text-2xs text-muted-foreground">
          {checks.length === 0
            ? "No checks yet"
            : failed.length > 0
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
      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        {groups.map(([groupId, rows]) => {
          const isCollapsed = collapsed.has(groupId);
          return (
            <section key={groupId}>
              <button
                className="flex w-full items-center gap-1.5 px-3 py-1.5 text-left text-2xs font-medium text-muted-foreground hover:text-foreground"
                onClick={() => toggleGroup(groupId)}
                type="button"
              >
                <span
                  aria-hidden="true"
                  className="w-2.5 text-muted-foreground/70"
                >
                  {isCollapsed ? "▸" : "▾"}
                </span>
                {CHECK_GROUP_LABEL[groupId]}
                <span className="tabular-nums text-muted-foreground/60">
                  ({rows.length})
                </span>
              </button>
              {isCollapsed
                ? null
                : rows.map((check, index) => {
                    const key = checkKey(check);
                    const open = expanded === key;
                    const failedRow = checkConclusionIsFailed(check.conclusion);
                    const duration = formatForgeCheckDuration(check);
                    return (
                      <div key={key}>
                        <div
                          className={cn(
                            "flex items-center gap-2 px-3 py-1.5 hover:bg-muted/30",
                            index < rows.length - 1 &&
                              !open &&
                              "border-b border-border/40",
                          )}
                        >
                          <button
                            className="flex min-w-0 flex-1 items-center gap-2.5 text-left"
                            data-testid={`thread-pr-hub-check-${check.name}`}
                            onClick={() => void loadTail(check)}
                            type="button"
                          >
                            <CheckCircleIcon group={groupId} />
                            <span className="min-w-0 flex-1 truncate text-sm text-foreground">
                              {displayForgeCheckName(check)}
                            </span>
                            <span className="shrink-0 tabular-nums text-2xs text-muted-foreground">
                              {duration ||
                                (groupId === "pending" ? "Queued" : "")}
                            </span>
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
                              "border-b border-border/40 bg-muted/20 px-3 py-2 pl-10 font-mono text-2xs",
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
          <p className="px-3 py-2 text-sm text-muted-foreground">
            No checks yet.
          </p>
        ) : null}
      </div>
    </div>
  );
}

/** Cursor-style circular status glyph. */
function CheckCircleIcon({ group }: { group: CheckGroupId }) {
  switch (group) {
    case "passed":
      return (
        <Check className="h-4 w-4 shrink-0 text-success" strokeWidth={2.5} />
      );
    case "failed":
      return (
        <X className="h-4 w-4 shrink-0 text-destructive" strokeWidth={2.5} />
      );
    case "skipped":
      return <CircleMinus className="h-4 w-4 shrink-0 text-muted-foreground" />;
    case "running":
      return (
        <LoaderCircle className="h-4 w-4 shrink-0 animate-spin text-attention" />
      );
    case "pending":
      return <Circle className="h-4 w-4 shrink-0 text-muted-foreground/70" />;
    default: {
      const _exhaustive: never = group;
      return _exhaustive;
    }
  }
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
