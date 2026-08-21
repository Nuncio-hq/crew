import { useState } from "react";

import { cn } from "@/shared/lib/cn";

import {
  CHECK_TONE_DOT,
  CHECK_TONE_TEXT,
  checkBucket,
  checkStateLabel,
  checkTone,
  summarizeCheckStates,
} from "./checkPresentation";

/**
 * Compact CI counts — prefer short glyphs when space is tight.
 * "3 ✓ · 1 ✗ · 2 ●" reads like Cursor; words stay in title.
 */
export function CiCheckSummary({
  className,
  failed,
  passed,
  running,
  total,
}: {
  className?: string;
  failed: number;
  passed: number;
  running: number;
  /** When set, show "N checks" when all zero / empty. */
  total?: number;
}) {
  if ((total ?? passed + failed + running) === 0) {
    return (
      <span
        className={cn("text-2xs text-muted-foreground", className)}
        data-testid="ci-check-summary"
      >
        No checks
      </span>
    );
  }

  return (
    <span
      className={cn(
        "inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-2xs tabular-nums",
        className,
      )}
      data-testid="ci-check-summary"
      title={`${passed} passed · ${failed} failed · ${running} running`}
    >
      <span className={CHECK_TONE_TEXT.success}>{passed} ✓</span>
      <span className={CHECK_TONE_TEXT.destructive}>{failed} ✗</span>
      {running > 0 ? (
        <span className={CHECK_TONE_TEXT.attention}>{running} ●</span>
      ) : null}
    </span>
  );
}

export function CiCheckSummaryFromStates({
  className,
  states,
}: {
  className?: string;
  states: readonly string[];
}) {
  const summary = summarizeCheckStates(states);
  return (
    <CiCheckSummary
      className={className}
      failed={summary.failed}
      passed={summary.passed}
      running={summary.running}
      total={states.length}
    />
  );
}

/** Colored +N −M · F files line. */
export function DiffStatSummary({
  additions,
  className,
  deletions,
  files,
}: {
  additions: number;
  className?: string;
  deletions: number;
  files: number | null;
}) {
  return (
    <span
      className={cn(
        "inline-flex flex-wrap items-center gap-x-1.5 font-mono text-sm tabular-nums",
        className,
      )}
      data-testid="diff-stat-summary"
    >
      <span className="text-success">+{additions}</span>
      <span className="text-destructive">−{deletions}</span>
      {files != null ? (
        <>
          <span aria-hidden="true" className="text-muted-foreground/40">
            ·
          </span>
          <span className="font-sans text-muted-foreground">
            {files} {files === 1 ? "file" : "files"}
          </span>
        </>
      ) : null}
    </span>
  );
}

export type TestRunDetailRow = {
  name: string;
  status: "passed" | "failed" | "running" | "pending" | "skipped";
};

/** Agent-claimed local test counts; expands to named rows like Cursor Checks. */
export function TestRunSummary({
  className,
  details,
  failed,
  passed,
  skipped,
}: {
  className?: string;
  /** Named tests or CI checks shown when the summary is expanded. */
  details?: readonly TestRunDetailRow[];
  failed: number;
  passed: number;
  skipped: number | null;
}) {
  const [open, setOpen] = useState(false);
  const tone = failed > 0 ? "destructive" : passed > 0 ? "success" : "muted";
  const rows = details ?? [];
  const canExpand = rows.length > 0;

  return (
    <div
      className={cn("rounded-md border border-border/50", className)}
      data-testid="test-run-summary"
    >
      <button
        aria-expanded={canExpand ? open : undefined}
        className={cn(
          "flex w-full items-center gap-2 px-2 py-1.5 text-left",
          canExpand
            ? "hover:bg-muted/40"
            : "cursor-default",
        )}
        data-testid="test-run-summary-toggle"
        disabled={!canExpand}
        onClick={() => {
          if (canExpand) setOpen((current) => !current);
        }}
        type="button"
      >
        <span
          aria-hidden="true"
          className="w-2.5 shrink-0 text-2xs text-muted-foreground/70"
        >
          {canExpand ? (open ? "▾" : "▸") : ""}
        </span>
        <span
          aria-hidden="true"
          className={cn(
            "inline-block h-1.5 w-1.5 shrink-0 rounded-full",
            tone === "success"
              ? "bg-success"
              : tone === "destructive"
                ? "bg-destructive"
                : "bg-muted-foreground",
          )}
        />
        <span
          className="sr-only"
          data-testid="test-run-summary-label"
        >{`${passed} passed, ${failed} failed`}</span>
        <span className="inline-flex min-w-0 flex-1 flex-wrap items-center gap-x-1.5 text-sm tabular-nums">
          <span
            className={passed > 0 ? "text-success" : "text-muted-foreground"}
          >
            {passed} passed
          </span>
          <span aria-hidden="true" className="text-muted-foreground/40">
            ·
          </span>
          <span
            className={failed > 0 ? "text-destructive" : "text-muted-foreground"}
          >
            {failed} failed
          </span>
          {skipped != null ? (
            <>
              <span aria-hidden="true" className="text-muted-foreground/40">
                ·
              </span>
              <span className="text-muted-foreground">{skipped} skipped</span>
            </>
          ) : null}
        </span>
      </button>
      {open && canExpand ? (
        <div
          className="border-t border-border/40 py-1"
          data-testid="test-run-summary-details"
        >
          <TestRunDetailGroups rows={rows} />
        </div>
      ) : null}
    </div>
  );
}

function TestRunDetailGroups({ rows }: { rows: readonly TestRunDetailRow[] }) {
  const groups: Array<{
    id: TestRunDetailRow["status"];
    label: string;
    items: TestRunDetailRow[];
  }> = [
    { id: "failed", label: "Failed", items: [] },
    { id: "running", label: "Running", items: [] },
    { id: "pending", label: "Pending", items: [] },
    { id: "passed", label: "Passed", items: [] },
    { id: "skipped", label: "Skipped", items: [] },
  ];
  for (const row of rows) {
    const group = groups.find((entry) => entry.id === row.status);
    group?.items.push(row);
  }

  return (
    <>
      {groups
        .filter((group) => group.items.length > 0)
        .map((group) => (
          <div key={group.id}>
            <p className="px-2 py-1 text-2xs font-medium text-muted-foreground">
              {group.label}
              <span className="ml-1 tabular-nums text-muted-foreground/60">
                ({group.items.length})
              </span>
            </p>
            <ul className="pb-1">
              {group.items.map((item) => (
                <li
                  className="flex items-center gap-2 px-2 py-1 text-sm"
                  key={`${item.status}:${item.name}`}
                >
                  <TestRunDetailGlyph status={item.status} />
                  <span className="min-w-0 flex-1 truncate text-foreground">
                    {item.name}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        ))}
    </>
  );
}

function TestRunDetailGlyph({
  status,
}: {
  status: TestRunDetailRow["status"];
}) {
  switch (status) {
    case "passed":
      return (
        <span aria-hidden="true" className="text-success">
          ✓
        </span>
      );
    case "failed":
      return (
        <span aria-hidden="true" className="text-destructive">
          ✗
        </span>
      );
    case "running":
      return (
        <span aria-hidden="true" className="text-attention">
          ●
        </span>
      );
    case "pending":
      return (
        <span aria-hidden="true" className="text-muted-foreground">
          ○
        </span>
      );
    case "skipped":
      return (
        <span aria-hidden="true" className="text-muted-foreground">
          –
        </span>
      );
    default: {
      const _exhaustive: never = status;
      return _exhaustive;
    }
  }
}

export function CheckStatusDot({
  className,
  state,
}: {
  className?: string;
  state: string;
}) {
  const tone = checkTone(state);
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-block h-2 w-2 shrink-0 rounded-full",
        CHECK_TONE_DOT[tone],
        className,
      )}
      data-bucket={checkBucket(state)}
    />
  );
}

export function CheckStatusLabel({
  className,
  state,
}: {
  className?: string;
  state: string;
}) {
  const tone = checkTone(state);
  return (
    <span className={cn("text-2xs", CHECK_TONE_TEXT[tone], className)}>
      {checkStateLabel(state)}
    </span>
  );
}
