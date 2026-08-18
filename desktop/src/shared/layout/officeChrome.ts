import { topChromeInset } from "@/shared/layout/chromeLayout";

/**
 * Office chrome tokens (#221). Wiki and Org reuse the same header bar,
 * field box, and composer surface the channel office already uses — not a
 * new look.
 */
export const OFFICE_SURFACE = {
  headerBar: "header-bar",
  fieldBox: "field-box",
  composerSurface: "composer-surface",
} as const;

export type OfficeSurface =
  (typeof OFFICE_SURFACE)[keyof typeof OFFICE_SURFACE];

/** Glass header strip — same backdrop as `TopChromeInsetHeader`, plus a bar edge. */
export const OFFICE_HEADER_BAR_CLASS = `${topChromeInset.headerBase} border-b border-border`;

/**
 * Visible field box. Full `border-input` (not `/40`) and `bg-card` so the
 * box lifts off the dialog/page; label stays outside this surface.
 */
export const OFFICE_FIELD_BOX_CLASS =
  "rounded-xl border border-input bg-card transition-colors duration-150 ease-out hover:border-muted-foreground/40 focus-within:border-muted-foreground/50";

/** Control inside a field box — no second border sitting on the label. */
export const OFFICE_FIELD_CONTROL_CLASS =
  "border-0 bg-transparent text-foreground shadow-none outline-none ring-0 transition-colors duration-150 ease-out placeholder:text-muted-foreground/55 focus:bg-transparent focus:text-foreground focus:outline-hidden focus-visible:ring-0";

export const OFFICE_FIELD_LABEL_CLASS =
  "text-2xs font-medium uppercase tracking-wide text-muted-foreground";

/** Ask / composer dock — rounded raised surface, not a flush page wash. */
export const OFFICE_COMPOSER_SURFACE_CLASS =
  "relative isolate rounded-2xl border border-input bg-card px-3 pb-2 pt-3 shadow-none";
