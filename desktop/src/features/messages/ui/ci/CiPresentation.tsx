import { cn } from "@/shared/lib/cn";

import {
  CHECK_TONE_DOT,
  CHECK_TONE_TEXT,
  checkBucket,
  checkStateLabel,
  checkTone,
  summarizeCheckStates,
  type CheckTone,
} from "./checkPresentation";

const TONE_LABEL: Record<CheckTone, string> = {
  success: "text-success",
  destructive: "text-destructive",
  attention: "text-attention",
  muted: "text-muted-foreground",
};

/**
 * Compact CI check counts: "3 passed · 1 failed · 2 running".
 * Used in forge summary, GitHub row, evidence cross-check — same vocabulary.
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
        No checks yet
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
    >
      <span className={CHECK_TONE_TEXT.success}>{passed} passed</span>
      <span aria-hidden="true" className="text-muted-foreground/40">
        ·
      </span>
      <span className={CHECK_TONE_TEXT.destructive}>{failed} failed</span>
      <span aria-hidden="true" className="text-muted-foreground/40">
        ·
      </span>
      <span className={CHECK_TONE_TEXT.attention}>{running} running</span>
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

/** Agent-claimed local test counts — not GitHub CI. */
export function TestRunSummary({
  className,
  failed,
  passed,
  skipped,
}: {
  className?: string;
  failed: number;
  passed: number;
  skipped: number | null;
}) {
  const tone: CheckTone =
    failed > 0 ? "destructive" : passed > 0 ? "success" : "muted";
  const label =
    failed > 0
      ? "Local tests failed"
      : passed > 0
        ? "Local tests passed"
        : "Local test claim";

  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-border/60 bg-background/40 px-2.5 py-2",
        className,
      )}
      data-testid="test-run-summary"
    >
      <span
        className={cn("text-2xs font-medium", TONE_LABEL[tone])}
        data-testid="test-run-summary-label"
      >
        {label}
      </span>
      <span className="inline-flex flex-wrap items-center gap-x-2 text-sm tabular-nums">
        <span className="text-success">{passed} passed</span>
        <span className="text-destructive">{failed} failed</span>
        {skipped != null ? (
          <span className="text-muted-foreground">{skipped} skipped</span>
        ) : null}
      </span>
    </div>
  );
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
