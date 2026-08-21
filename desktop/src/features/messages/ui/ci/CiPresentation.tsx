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

/** Agent-claimed local test counts — not GitHub CI. Cursor-compact: one row. */
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
  const tone = failed > 0 ? "destructive" : passed > 0 ? "success" : "muted";

  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-x-2 gap-y-0.5 rounded-md border border-border/50 px-2 py-1.5",
        className,
      )}
      data-testid="test-run-summary"
    >
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
      <span className="inline-flex flex-wrap items-center gap-x-1.5 text-sm tabular-nums">
        <span className={passed > 0 ? "text-success" : "text-muted-foreground"}>
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
