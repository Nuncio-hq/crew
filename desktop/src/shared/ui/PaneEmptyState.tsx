import type { ReactNode } from "react";

import { AUXILIARY_PANEL_NARROW_PX } from "@/shared/layout/responsiveContract";
import { cn } from "@/shared/lib/cn";

/**
 * Width-aware empty state (#205 P6).
 *
 * Standard: title + optional supporting paragraph.
 * Narrow (container ≤340px): icon + one short line; no paragraphs.
 */
export function PaneEmptyState({
  className,
  description,
  icon,
  narrowTitle,
  testId,
  title,
}: {
  className?: string;
  description?: string;
  icon?: ReactNode;
  narrowTitle?: string;
  testId?: string;
  title: string;
}) {
  const shortTitle = narrowTitle ?? title;
  return (
    <div
      className={cn(
        "@container w-full min-w-0 rounded-2xl border border-dashed border-border/70 bg-card/40 px-4 py-6 text-center",
        className,
      )}
      data-narrow-below={AUXILIARY_PANEL_NARROW_PX}
      data-testid={testId}
    >
      {icon ? (
        <div className="mx-auto mb-2 flex justify-center text-muted-foreground/60">
          {icon}
        </div>
      ) : null}
      <p className="truncate text-sm font-medium text-foreground/80 [@container(max-width:21.25rem)]:hidden">
        {title}
      </p>
      <p className="hidden truncate text-sm font-medium text-foreground/80 [@container(max-width:21.25rem)]:block">
        {shortTitle}
      </p>
      {description ? (
        <p className="mt-1 text-xs text-muted-foreground [@container(max-width:21.25rem)]:hidden">
          {description}
        </p>
      ) : null}
    </div>
  );
}
