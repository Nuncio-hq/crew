/**
 * Desktop pane responsive contract (#205 / D-064).
 *
 * Panes resize independently of the window, so components adapt to **their own
 * width** (CSS `@container` / ResizeObserver), never to the viewport — except
 * true window-level layout (sidebar collapse; app floor 800×500).
 *
 * Below a surface's minimum a component must **truncate**, **collapse**, or
 * **stack**. Squeezing (vertical letter-soup, overlapping chrome) is never an
 * option.
 *
 * Truncation vocabulary: titles = end-ellipsis; branches/paths/URLs =
 * middle-truncate; counts/badges never truncate (they collapse). Tooltips
 * carry the full value on truncated elements.
 *
 * Stacking order at narrow widths: header → status strip → content → input.
 * Overlay rails (declared plans, governor) insert **between header and
 * content**, pushing content down — never covering it.
 */

import {
  AUXILIARY_PANEL_DEFAULT_WIDTH_PX,
  AUXILIARY_PANEL_MAX_WIDTH_PX,
  AUXILIARY_PANEL_MIN_WIDTH_PX,
} from "@/shared/layout/auxiliaryPanelLayout";

/** App window floor from `desktop/src-tauri/tauri.conf.json`. */
export const APP_WINDOW_MIN_WIDTH_PX = 800;
export const APP_WINDOW_MIN_HEIGHT_PX = 500;

/** Sidebar contract width (#203). Rows truncate; badges never wrap. */
export const SIDEBAR_CONTRACT_WIDTH_PX = 256;
/** Hide work-thread line 2 (branch · PR · CI) below this sidebar width. */
export const SIDEBAR_META_COLLAPSE_BELOW_PX = 220;
export const SIDEBAR_WIDTH_MIN_PX = 220;
export const SIDEBAR_WIDTH_MAX_PX = 420;

/**
 * Auxiliary / thread panel clamp (existing policy — imported, not restated).
 */
export const AUXILIARY_PANEL_CONTRACT_MIN_PX = AUXILIARY_PANEL_MIN_WIDTH_PX;
export const AUXILIARY_PANEL_CONTRACT_MAX_PX = AUXILIARY_PANEL_MAX_WIDTH_PX;
export { AUXILIARY_PANEL_DEFAULT_WIDTH_PX };
/**
 * Narrow-pane threshold: meta rows collapse to icons; empty states use the
 * one-line variant; declared-plans rail is guaranteed stacked.
 */
export const AUXILIARY_PANEL_NARROW_PX = 340;

/** Declared-plans side rail is `w-72` (18rem). */
export const DECLARED_PLANS_RAIL_WIDTH_PX = 288;
/**
 * Side-by-side rail + remaining content. Below this, the rail **stacks**
 * under the header so the thread body cannot squeeze to letter-soup.
 * 288px rail + 220px min readable column.
 */
export const DECLARED_PLANS_SIDE_BY_SIDE_MIN_PX =
  DECLARED_PLANS_RAIL_WIDTH_PX + SIDEBAR_META_COLLAPSE_BELOW_PX;

/** Focus drawer: channel sliver + column clamp (existing policy). */
export const FOCUS_DRAWER_SLIVER_WIDTH_PX = 72;
export const FOCUS_DRAWER_COLUMN_CLAMP_PX = 880;
/** Focus two-pane (chat | tools) collapse — existing `#193`/`#196` toggle. */
export const FOCUS_DRAWER_SPLIT_COLLAPSE_PX = 1100;

/** Tool Pane (#196) minimum. Tabs become icon-only; control bars wrap. */
export const TOOL_PANE_MIN_WIDTH_PX = 300;
export const TOOL_PANE_LABEL_MIN_PX = 360;

/** PR hub file-tree + diff. Below this the tree collapses to a dropdown. */
export const PR_HUB_SPLIT_MIN_PX = 520;

/** Wiki TOC rail. Below this it collapses to a hamburger over content. */
export const WIKI_TOC_MIN_WIDTH_PX = 200;
export const WIKI_TOC_COLLAPSE_BELOW_PX = 520;

/** Huddle companion window (existing native mins — do not change). */
export const HUDDLE_COMPANION_MIN_WIDTH_PX = 720;
export const HUDDLE_COMPANION_MIN_HEIGHT_PX = 520;

/** Letter-soup floor used by E2E `assertPaneResponsive`. */
export const LETTER_SOUP_MIN_CH = 6;

export type BelowMinBehavior = "truncate" | "collapse" | "stack";

export type ResponsiveSurfaceContract = {
  readonly behavior: BelowMinBehavior | "floor";
  readonly minWidthPx: number;
  readonly surface: string;
};

/**
 * Per-surface table. Every new pane must add a row (definition of done).
 * Values encode existing policy; they are not a license to redesign mins.
 */
export const RESPONSIVE_SURFACE_CONTRACT: readonly ResponsiveSurfaceContract[] =
  [
    {
      surface: "App window",
      minWidthPx: APP_WINDOW_MIN_WIDTH_PX,
      behavior: "floor",
    },
    {
      surface: "Sidebar",
      minWidthPx: SIDEBAR_CONTRACT_WIDTH_PX,
      behavior: "truncate",
    },
    {
      surface: "Auxiliary/thread panel",
      minWidthPx: AUXILIARY_PANEL_CONTRACT_MIN_PX,
      behavior: "stack",
    },
    {
      surface: "Focus drawer",
      minWidthPx: FOCUS_DRAWER_SLIVER_WIDTH_PX,
      behavior: "collapse",
    },
    {
      surface: "Tool Pane",
      minWidthPx: TOOL_PANE_MIN_WIDTH_PX,
      behavior: "collapse",
    },
    {
      surface: "PR hub",
      minWidthPx: PR_HUB_SPLIT_MIN_PX,
      behavior: "collapse",
    },
    {
      surface: "Wiki TOC rail",
      minWidthPx: WIKI_TOC_MIN_WIDTH_PX,
      behavior: "collapse",
    },
    {
      surface: "Composer",
      minWidthPx: AUXILIARY_PANEL_CONTRACT_MIN_PX,
      behavior: "collapse",
    },
    {
      surface: "Huddle companion",
      minWidthPx: HUDDLE_COMPANION_MIN_WIDTH_PX,
      behavior: "floor",
    },
  ];

export function shouldStackDeclaredPlansRail(paneWidthPx: number): boolean {
  return paneWidthPx < DECLARED_PLANS_SIDE_BY_SIDE_MIN_PX;
}

export function isNarrowPane(widthPx: number): boolean {
  return widthPx > 0 && widthPx <= AUXILIARY_PANEL_NARROW_PX;
}
