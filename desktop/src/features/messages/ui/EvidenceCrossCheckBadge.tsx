import type { EvidenceCrossCheckResult } from "@/features/messages/lib/evidenceCrossCheck";
import { cn } from "@/shared/lib/cn";

const CHIP: Record<EvidenceCrossCheckResult["state"], string> = {
  matches: "border-success/30 bg-success/10 text-success",
  diverges: "border-attention/40 bg-attention/15 text-attention",
  "ci-running": "border-border/70 bg-muted/40 text-muted-foreground",
  "not-comparable": "border-border/50 bg-muted/20 text-muted-foreground/80",
};

/**
 * Header chip for evidence↔CI consistency. Informational only — never a gate.
 * Cursor-style: short label, detail lives in the title tooltip / compact row.
 */
export function EvidenceCrossCheckBadge({
  result,
}: {
  result: EvidenceCrossCheckResult;
}) {
  return (
    <div className="min-w-0 max-w-[10rem] text-right">
      <span
        className={cn(
          "inline-flex items-center rounded-md border px-1.5 py-0.5 text-2xs font-medium",
          CHIP[result.state],
        )}
        data-testid="evidence-cross-check-badge"
        data-state={result.state}
        role="status"
        title={result.detail ?? result.label}
      >
        {result.label}
      </span>
    </div>
  );
}

/**
 * Compact diverge callout — one short line, no lecture.
 */
export function EvidenceCrossCheckDetail({
  result,
}: {
  result: EvidenceCrossCheckResult;
}) {
  if (result.state !== "diverges" || !result.detail) return null;
  return (
    <p
      className="mb-2 truncate font-mono text-2xs text-attention"
      data-testid="evidence-cross-check-detail"
      title={result.detail}
    >
      {result.detail}
    </p>
  );
}
