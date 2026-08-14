/**
 * First-party Crew chrome themes. Chrome (app shell) is independent of
 * Shiki syntax palettes — see D-063.
 */

export const CREW_DARK_THEME_NAME = "crew-dark";
export const CREW_LIGHT_THEME_NAME = "crew-light";

export const CHROME_THEMES = [
  CREW_DARK_THEME_NAME,
  CREW_LIGHT_THEME_NAME,
] as const;

export type ChromeThemeName = (typeof CHROME_THEMES)[number];

export const DEFAULT_CHROME_THEME: ChromeThemeName = CREW_DARK_THEME_NAME;

export const CREW_ACCENT_HEX = "#3b82f6";
export const CREW_LIGHT_ACCENT_HEX = "#2563eb";

export function isChromeThemeName(
  name: string | null | undefined,
): name is ChromeThemeName {
  return (
    typeof name === "string" &&
    (CHROME_THEMES as readonly string[]).includes(name)
  );
}

export function isLightChromeTheme(name: string): boolean {
  return name === CREW_LIGHT_THEME_NAME;
}

export function getChromePair(name: ChromeThemeName): ChromeThemeName {
  return name === CREW_DARK_THEME_NAME
    ? CREW_LIGHT_THEME_NAME
    : CREW_DARK_THEME_NAME;
}

export function resolveChromeSystemTheme(
  selected: ChromeThemeName,
  systemIsDark: boolean,
): ChromeThemeName {
  const selectedIsLight = isLightChromeTheme(selected);
  const needsSwitch =
    (systemIsDark && selectedIsLight) || (!systemIsDark && !selectedIsLight);
  return needsSwitch ? getChromePair(selected) : selected;
}

export function chromeThemeLabel(name: ChromeThemeName): string {
  return name === CREW_DARK_THEME_NAME ? "Crew Dark" : "Crew Light";
}
