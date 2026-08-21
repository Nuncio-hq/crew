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
  ForgeCheckLogTail,
} from "@/shared/api/threadForgeTypes";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import { CheckStatusDot, CiCheckSummaryFromStates } from "../ci/CiPresentation";
import { checkConclusionIsFailed } from "./forgeHubCopy";

export function ThreadPrHubChecks({
  checks,
  onRefresh,
  refIdentity,
}: {
  checks: ForgeCheck[];
  onRefresh: () => void;
  refIdentity: Extract<ThreadForgeHubSubject, { kind: "pr" }>;
}) {
  const groups = React.useMemo(() => groupByWorkflow(checks), [checks]);
  const [expanded, setExpanded] = React.useState<string | null>(null);
  const [tails, setTails] = React.useState<ForgeCheckLogTail[] | null>(null);
  const [busyRun, setBusyRun] = React.useState<number | null>(null);

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

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden"
      data-testid="thread-pr-hub-checks"
    >
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 px-3 py-2">
        <div className="min-w-0">
          <p className="text-2xs font-medium text-muted-foreground">
            GitHub checks
          </p>
          <CiCheckSummaryFromStates
            states={checks.map((check) => check.conclusion)}
          />
        </div>
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
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {groups.map(([workflow, rows]) => (
          <section className="mb-4" key={workflow}>
            <h3 className="mb-1 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
              {workflow}
            </h3>
            {rows.map((check) => {
              const key = checkKey(check);
              const open = expanded === key;
              const failedRow = checkConclusionIsFailed(check.conclusion);
              return (
                <div
                  className="mb-1 rounded-md border border-border/60"
                  key={key}
                >
                  <div className="flex items-start gap-2 px-2 py-1.5">
                    <button
                      className="min-w-0 flex-1 text-left"
                      data-testid={`thread-pr-hub-check-${check.name}`}
                      onClick={() => void loadTail(check)}
                      type="button"
                    >
                      <div className="flex items-center gap-2">
                        <CheckStatusDot state={check.conclusion} />
                        <span className="truncate text-sm">{check.name}</span>
                      </div>
                      <p className="font-mono text-2xs text-muted-foreground">
                        {check.status}
                        {check.conclusion !== "unknown"
                          ? ` · ${check.conclusion}`
                          : ""}
                      </p>
                    </button>
                    {check.url ? (
                      <a
                        className="shrink-0 text-muted-foreground hover:text-foreground"
                        href={check.url}
                        rel="noreferrer"
                        target="_blank"
                        title="Open on the forge"
                      >
                        <ExternalLink className="h-3.5 w-3.5" />
                      </a>
                    ) : null}
                    {check.runId ? (
                      <Button
                        disabled={busyRun === check.runId}
                        onClick={() => void rerun(check.runId as number, false)}
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
                        "border-t border-border/60 bg-muted/30 p-2 font-mono text-2xs",
                        failedRow
                          ? "text-destructive"
                          : "text-muted-foreground",
                      )}
                      data-testid="thread-pr-hub-check-log"
                    >
                      {tails === null ? (
                        <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                      ) : tails.length === 0 ? (
                        <p>No failed-step log for this check.</p>
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
        ))}
        {checks.length === 0 ? (
          <p className="text-sm text-muted-foreground">No checks yet.</p>
        ) : null}
      </div>
    </div>
  );
}

function groupByWorkflow(checks: ForgeCheck[]): Array<[string, ForgeCheck[]]> {
  const map = new Map<string, ForgeCheck[]>();
  for (const check of checks) {
    const key = check.workflow?.trim() || "Other";
    const list = map.get(key) ?? [];
    list.push(check);
    map.set(key, list);
  }
  return [...map.entries()];
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
