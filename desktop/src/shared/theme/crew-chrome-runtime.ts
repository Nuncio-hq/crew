import { getStorageItem } from "@/shared/lib/safeStorage";
import {
  CREW_ACCENT_HEX,
  CREW_DARK_THEME_NAME,
  CREW_LIGHT_ACCENT_HEX,
  type ChromeThemeName,
} from "./chrome-theme";
import { applyCrewTokenVars, type CrewTokenVars } from "./crew-tokens";
import { hexToHsl } from "./adaptive-theme";
import {
  extractThemeInfo,
  loadThemeData,
  type SyntaxThemeName,
  type ThemeInfo,
} from "./theme-loader";

export const THEME_CACHE_KEY = "buzz-theme-cache";

function contrastForeground(hex: string): string {
  const match = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})/i.exec(hex);
  if (!match) return "#ffffff";
  const r = parseInt(match[1], 16);
  const g = parseInt(match[2], 16);
  const b = parseInt(match[3], 16);
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return lum > 0.5 ? "#000000" : "#ffffff";
}

/** Pin Crew chrome to the one blue accent; do not paint user swatches. */
export function applyCrewAccent(chrome: ChromeThemeName) {
  const hex =
    chrome === CREW_DARK_THEME_NAME ? CREW_ACCENT_HEX : CREW_LIGHT_ACCENT_HEX;
  const root = document.documentElement;
  const accentHsl = hexToHsl(hex);
  const fgHsl = hexToHsl(contrastForeground(hex));
  root.style.setProperty("--buzz-selected-accent", accentHsl);
  root.style.setProperty("--primary", accentHsl);
  root.style.setProperty("--primary-foreground", fgHsl);
  root.style.setProperty("--sidebar-primary", accentHsl);
  root.style.setProperty("--sidebar-primary-foreground", fgHsl);
  root.style.setProperty("--ring", accentHsl);
}

export function applyCrewChromeDocument(chrome: ChromeThemeName): {
  isDark: boolean;
  vars: CrewTokenVars;
} {
  const root = document.documentElement;
  const vars = applyCrewTokenVars(root.style, chrome);
  const isDark = chrome === CREW_DARK_THEME_NAME;
  root.classList.remove("light", "dark");
  root.classList.add(isDark ? "dark" : "light");
  root.setAttribute("data-crew-chrome", chrome);
  root.removeAttribute("data-buzz-sidebar");
  root.removeAttribute("data-buzz-theme");
  applyCrewAccent(chrome);
  return { isDark, vars };
}

export function writeThemeCache(input: {
  themeName: ChromeThemeName;
  syntaxThemeName: SyntaxThemeName;
  vars: CrewTokenVars;
  isDark: boolean;
}) {
  try {
    window.localStorage.setItem(THEME_CACHE_KEY, JSON.stringify(input));
  } catch {
    // Storage full — non-critical
  }
}

export function readThemeCache(): {
  themeName: string;
  syntaxThemeName?: string;
  vars: Record<string, string>;
  isDark: boolean;
} | null {
  try {
    const cached = window.localStorage.getItem(THEME_CACHE_KEY);
    if (!cached) return null;
    return JSON.parse(cached) as {
      themeName: string;
      syntaxThemeName?: string;
      vars: Record<string, string>;
      isDark: boolean;
    };
  } catch {
    return null;
  }
}

export async function loadSyntaxTerminalPalette(
  syntax: SyntaxThemeName,
): Promise<ThemeInfo["terminalPalette"] | null> {
  try {
    const themeData = await loadThemeData(syntax);
    return extractThemeInfo(syntax, themeData).terminalPalette;
  } catch {
    return null;
  }
}

export { getStorageItem };
