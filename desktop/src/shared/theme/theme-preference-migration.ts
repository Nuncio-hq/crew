/**
 * One-time chrome/syntax split for stored appearance prefs.
 *
 * Old `buzz-theme` held a Shiki palette (or buzz/buzz-dark) that painted the
 * whole app. After the split, chrome is Crew Dark/Light and syntax is a
 * Shiki name (default dark-plus).
 */

import {
  type ChromeThemeName,
  DEFAULT_CHROME_THEME,
  chromeThemeLabel,
  isChromeThemeName,
  isLightChromeTheme,
} from "./chrome-theme";
import {
  LIGHT_THEMES,
  SYNTAX_THEMES,
  type SyntaxThemeName,
} from "./theme-loader";

export const SYNTAX_STORAGE_KEY = "buzz-syntax-theme";
export const THEME_SPLIT_MIGRATION_KEY = "buzz-theme-split-v1";
export const DEFAULT_SYNTAX_THEME: SyntaxThemeName = "dark-plus";

const SHIKI_PALETTE_NAMES = new Set<string>(
  (SYNTAX_THEMES as readonly string[]).filter(
    (name) => name !== "buzz" && name !== "buzz-dark",
  ),
);

export function isShikiPaletteName(name: string): name is SyntaxThemeName {
  return SHIKI_PALETTE_NAMES.has(name);
}

export function formatSyntaxThemeLabel(name: string): string {
  return name
    .split("-")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

export type MigratedAppearance = {
  chrome: ChromeThemeName;
  syntax: SyntaxThemeName;
  migratedFromLegacy: boolean;
  toastMessage: string | null;
};

function chromeFromLegacyThemeName(name: string): ChromeThemeName {
  if (isChromeThemeName(name)) return name;
  if (name === "light") return "crew-light";
  if (name === "dark" || name === "system") return "crew-dark";
  return LIGHT_THEMES.has(name as SyntaxThemeName) ? "crew-light" : "crew-dark";
}

function syntaxFromLegacyThemeName(
  name: string,
  storedSyntax: string | null,
): SyntaxThemeName {
  if (storedSyntax && isShikiPaletteName(storedSyntax)) return storedSyntax;
  if (isShikiPaletteName(name)) return name;
  return DEFAULT_SYNTAX_THEME;
}

export function migrateStoredAppearance(input: {
  storedTheme: string | null;
  storedSyntax: string | null;
  alreadyMigrated: boolean;
}): MigratedAppearance {
  if (input.alreadyMigrated) {
    const chrome = isChromeThemeName(input.storedTheme)
      ? input.storedTheme
      : DEFAULT_CHROME_THEME;
    const syntax =
      input.storedSyntax && isShikiPaletteName(input.storedSyntax)
        ? input.storedSyntax
        : DEFAULT_SYNTAX_THEME;
    return {
      chrome,
      syntax,
      migratedFromLegacy: false,
      toastMessage: null,
    };
  }

  const stored = input.storedTheme;
  if (!stored) {
    return {
      chrome: DEFAULT_CHROME_THEME,
      syntax:
        input.storedSyntax && isShikiPaletteName(input.storedSyntax)
          ? input.storedSyntax
          : DEFAULT_SYNTAX_THEME,
      migratedFromLegacy: false,
      toastMessage: null,
    };
  }

  if (isChromeThemeName(stored)) {
    return {
      chrome: stored,
      syntax:
        input.storedSyntax && isShikiPaletteName(input.storedSyntax)
          ? input.storedSyntax
          : DEFAULT_SYNTAX_THEME,
      migratedFromLegacy: false,
      toastMessage: null,
    };
  }

  const chrome = chromeFromLegacyThemeName(stored);
  const syntax = syntaxFromLegacyThemeName(stored, input.storedSyntax);
  const keptSyntax = isShikiPaletteName(stored);
  const toastMessage = keptSyntax
    ? `Your theme is now ${chromeThemeLabel(chrome)}; code blocks kept ${formatSyntaxThemeLabel(stored)}`
    : `Your theme is now ${chromeThemeLabel(chrome)}; code blocks use ${formatSyntaxThemeLabel(syntax)}`;

  return {
    chrome,
    syntax,
    migratedFromLegacy: true,
    toastMessage,
  };
}

export function normalizeChromeThemeName(
  name: string | null | undefined,
): ChromeThemeName {
  if (name && isChromeThemeName(name)) return name;
  if (!name) return DEFAULT_CHROME_THEME;
  return chromeFromLegacyThemeName(name);
}

export function chromeLuminanceFromLegacyName(name: string): ChromeThemeName {
  return chromeFromLegacyThemeName(name);
}

export { isLightChromeTheme };
