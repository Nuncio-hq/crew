/**
 * Crew Dark / Crew Light token maps.
 *
 * Values are calibrated against Cursor Dark (near-black neutrals, one blue
 * accent) with WCAG AA on all four surfaces. Indicative issue opacities
 * (meta white/40%) were raised so meta text stays ≥ 4.5:1 on overlay.
 *
 * Color is information: chrome is achromatic; these maps reserve hue for
 * the accent and the semantic state set.
 */

import type { ChromeThemeName } from "./chrome-theme";
import {
  CREW_ACCENT_HEX,
  CREW_DARK_THEME_NAME,
  CREW_LIGHT_ACCENT_HEX,
} from "./chrome-theme";

export type CrewTokenVars = Record<string, string>;

/** Hex surfaces / text used by contrast tests and the literal-color allowlist. */
export const CREW_DARK_HEX = {
  shell: "#141414",
  content: "#181818",
  raised: "#1f1f1f",
  overlay: "#262626",
  border: "#2a2a2a",
  borderStrong: "#383838",
  hover: "#262626",
  selected: "#2f2f2f",
  foreground: "#ededed",
  secondary: "#acacac",
  meta: "#979797",
  accent: CREW_ACCENT_HEX,
  primaryForeground: "#ffffff",
  success: "#3fb950",
  danger: "#f85149",
  attention: "#d29922",
  merged: "#a371f7",
} as const;

export const CREW_LIGHT_HEX = {
  shell: "#f2f2f2",
  content: "#fafafa",
  raised: "#ffffff",
  overlay: "#ffffff",
  border: "#ebebeb",
  borderStrong: "#dbdbdb",
  hover: "#eeeeee",
  selected: "#e3e3e3",
  foreground: "#141414",
  secondary: "#5c5c5c",
  meta: "#6a6a6a",
  accent: CREW_LIGHT_ACCENT_HEX,
  primaryForeground: "#ffffff",
  success: "#1a7f37",
  danger: "#cf222e",
  attention: "#8a5a00",
  merged: "#8250df",
} as const;

function hsl(h: string): string {
  return h;
}

/** shadcn HSL component strings ("H S% L%"). */
export const CREW_DARK_VARS: CrewTokenVars = {
  "--background": hsl("0 0% 9.4%"),
  "--card": hsl("0 0% 12.2%"),
  "--popover": hsl("0 0% 14.9%"),
  "--muted": hsl("0 0% 12.2%"),
  "--accent": hsl("0 0% 14.9%"),
  "--secondary": hsl("0 0% 12.2%"),
  "--foreground": hsl("0 0% 92.9%"),
  "--card-foreground": hsl("0 0% 92.9%"),
  "--popover-foreground": hsl("0 0% 92.9%"),
  "--muted-foreground": hsl("0 0% 67.5%"),
  "--meta-foreground": hsl("0 0% 59.2%"),
  "--accent-foreground": hsl("0 0% 92.9%"),
  "--secondary-foreground": hsl("0 0% 92.9%"),
  "--primary": hsl("217.2 91.22% 59.8%"),
  "--primary-foreground": hsl("0 0% 100%"),
  "--destructive": hsl("2.7 92.59% 62.9%"),
  "--destructive-foreground": hsl("0 0% 9.4%"),
  "--success": hsl("128.4 49.19% 48.6%"),
  "--success-foreground": hsl("0 0% 9.4%"),
  "--attention": hsl("40.6 72.13% 47.8%"),
  "--attention-foreground": hsl("0 0% 9.4%"),
  "--merged": hsl("262.4 89.33% 70.6%"),
  "--merged-foreground": hsl("0 0% 9.4%"),
  "--border": hsl("0 0% 16.5%"),
  "--border-strong": hsl("0 0% 22.0%"),
  "--input": hsl("0 0% 16.5%"),
  "--ring": hsl("217.2 91.22% 59.8%"),
  "--sidebar": hsl("0 0% 7.8%"),
  "--sidebar-background": hsl("0 0% 7.8%"),
  "--sidebar-foreground": hsl("0 0% 92.9%"),
  "--sidebar-primary": hsl("217.2 91.22% 59.8%"),
  "--sidebar-primary-foreground": hsl("0 0% 100%"),
  "--sidebar-active": hsl("0 0% 18.4%"),
  "--sidebar-active-foreground": hsl("0 0% 92.9%"),
  "--sidebar-accent": hsl("0 0% 18.4%"),
  "--sidebar-accent-foreground": hsl("0 0% 92.9%"),
  "--sidebar-border": hsl("0 0% 16.5%"),
  "--sidebar-ring": hsl("217.2 91.22% 59.8%"),
  "--huddle-drawer-surface": hsl("0 0% 14.9%"),
  "--huddle-control-surface": hsl("0 0% 18.4%"),
  "--huddle-control-hover-surface": hsl("0 0% 22.0%"),
  "--huddle-control-chevron-surface": hsl("0 0% 12.2%"),
  "--huddle-control-chevron-hover-surface": hsl("0 0% 16.5%"),
  "--huddle-control-foreground": hsl("0 0% 92.9%"),
  "--huddle-popover-surface": hsl("0 0% 14.9%"),
  "--huddle-popover-border": hsl("0 0% 22.0%"),
  "--huddle-tooltip-surface": hsl("0 0% 18.4%"),
  "--huddle-tooltip-foreground": hsl("0 0% 92.9%"),
  "--chart-1": hsl("128.4 49.19% 48.6%"),
  "--chart-2": hsl("2.7 92.59% 62.9%"),
  "--chart-3": hsl("40.6 72.13% 47.8%"),
  "--chart-4": hsl("262.4 89.33% 70.6%"),
  "--chart-5": hsl("0 0% 59.2%"),
  "--status-added": CREW_DARK_HEX.success,
  "--status-deleted": CREW_DARK_HEX.danger,
  "--status-modified": CREW_DARK_HEX.attention,
  "--ui-warning": CREW_DARK_HEX.attention,
  "--ui-warning-bg": "rgba(210, 153, 34, 0.12)",
};

export const CREW_LIGHT_VARS: CrewTokenVars = {
  "--background": hsl("0 0% 98.0%"),
  "--card": hsl("0 0% 100.0%"),
  "--popover": hsl("0 0% 100.0%"),
  "--muted": hsl("0 0% 94.9%"),
  "--accent": hsl("0 0% 93.3%"),
  "--secondary": hsl("0 0% 94.9%"),
  "--foreground": hsl("0 0% 7.8%"),
  "--card-foreground": hsl("0 0% 7.8%"),
  "--popover-foreground": hsl("0 0% 7.8%"),
  "--muted-foreground": hsl("0 0% 36.1%"),
  "--meta-foreground": hsl("0 0% 41.6%"),
  "--accent-foreground": hsl("0 0% 7.8%"),
  "--secondary-foreground": hsl("0 0% 7.8%"),
  "--primary": hsl("221.2 83.19% 53.3%"),
  "--primary-foreground": hsl("0 0% 100%"),
  "--destructive": hsl("355.8 71.78% 47.3%"),
  "--destructive-foreground": hsl("0 0% 100%"),
  "--success": hsl("137.2 66.01% 30.0%"),
  "--success-foreground": hsl("0 0% 100%"),
  "--attention": hsl("39.1 100.00% 27.1%"),
  "--attention-foreground": hsl("0 0% 100%"),
  "--merged": hsl("261.0 69.08% 59.4%"),
  "--merged-foreground": hsl("0 0% 100%"),
  "--border": hsl("0 0% 92.2%"),
  "--border-strong": hsl("0 0% 85.9%"),
  "--input": hsl("0 0% 92.2%"),
  "--ring": hsl("221.2 83.19% 53.3%"),
  "--sidebar": hsl("0 0% 94.9%"),
  "--sidebar-background": hsl("0 0% 94.9%"),
  "--sidebar-foreground": hsl("0 0% 7.8%"),
  "--sidebar-primary": hsl("221.2 83.19% 53.3%"),
  "--sidebar-primary-foreground": hsl("0 0% 100%"),
  "--sidebar-active": hsl("0 0% 89.0%"),
  "--sidebar-active-foreground": hsl("0 0% 7.8%"),
  "--sidebar-accent": hsl("0 0% 89.0%"),
  "--sidebar-accent-foreground": hsl("0 0% 7.8%"),
  "--sidebar-border": hsl("0 0% 92.2%"),
  "--sidebar-ring": hsl("221.2 83.19% 53.3%"),
  "--huddle-drawer-surface": hsl("0 0% 100.0%"),
  "--huddle-control-surface": hsl("0 0% 94.9%"),
  "--huddle-control-hover-surface": hsl("0 0% 89.0%"),
  "--huddle-control-chevron-surface": hsl("0 0% 92.2%"),
  "--huddle-control-chevron-hover-surface": hsl("0 0% 85.9%"),
  "--huddle-control-foreground": hsl("0 0% 7.8%"),
  "--huddle-popover-surface": hsl("0 0% 100.0%"),
  "--huddle-popover-border": hsl("0 0% 85.9%"),
  "--huddle-tooltip-surface": hsl("0 0% 94.9%"),
  "--huddle-tooltip-foreground": hsl("0 0% 7.8%"),
  "--chart-1": hsl("137.2 66.01% 30.0%"),
  "--chart-2": hsl("355.8 71.78% 47.3%"),
  "--chart-3": hsl("39.1 100.00% 27.1%"),
  "--chart-4": hsl("261.0 69.08% 59.4%"),
  "--chart-5": hsl("0 0% 45.9%"),
  "--status-added": CREW_LIGHT_HEX.success,
  "--status-deleted": CREW_LIGHT_HEX.danger,
  "--status-modified": CREW_LIGHT_HEX.attention,
  "--ui-warning": CREW_LIGHT_HEX.attention,
  "--ui-warning-bg": "rgba(138, 90, 0, 0.10)",
};

export function crewTokenVars(chrome: ChromeThemeName): CrewTokenVars {
  return chrome === CREW_DARK_THEME_NAME ? CREW_DARK_VARS : CREW_LIGHT_VARS;
}

export function crewHexPalette(
  chrome: ChromeThemeName,
): Record<keyof typeof CREW_DARK_HEX, string> {
  return chrome === CREW_DARK_THEME_NAME ? CREW_DARK_HEX : CREW_LIGHT_HEX;
}

export function applyCrewTokenVars(
  root: CSSStyleDeclaration,
  chrome: ChromeThemeName,
): CrewTokenVars {
  const vars = crewTokenVars(chrome);
  for (const [key, value] of Object.entries(vars)) {
    root.setProperty(key, value);
  }
  return vars;
}
