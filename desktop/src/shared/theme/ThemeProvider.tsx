import {
  type ReactNode,
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { toast } from "sonner";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invokeTauri } from "@/shared/api/tauri";
import { isMacPlatform } from "@/shared/lib/platform";
import { getStorageItem } from "@/shared/lib/safeStorage";
import { hexToHsl } from "./adaptive-theme";
import {
  CREW_ACCENT_HEX,
  CREW_DARK_THEME_NAME,
  CREW_LIGHT_ACCENT_HEX,
  DEFAULT_CHROME_THEME,
  type ChromeThemeName,
  isChromeThemeName,
  resolveChromeSystemTheme,
} from "./chrome-theme";
import {
  applyCrewChromeDocument,
  loadSyntaxTerminalPalette,
  readThemeCache,
  writeThemeCache,
} from "./crew-chrome-runtime";
import {
  SYNTAX_STORAGE_KEY,
  THEME_SPLIT_MIGRATION_KEY,
  isShikiPaletteName,
  migrateStoredAppearance,
  normalizeChromeThemeName,
} from "./theme-preference-migration";
import type { SyntaxThemeName, ThemeInfo } from "./theme-loader";

export const THEME_STORAGE_KEY = "buzz-theme";
export const SYNTAX_THEME_STORAGE_KEY = SYNTAX_STORAGE_KEY;
export const ACCENT_STORAGE_KEY = "buzz-accent-color";
export const GLASS_BACKGROUND_STORAGE_KEY = "buzz-glass-background";
export const GLASS_OPACITY_STORAGE_KEY = "buzz-glass-opacity";
export const PROMINENT_ACTIVE_TAB_STORAGE_KEY = "buzz-prominent-active-tab";
export const GLASS_OPACITY_MIN = 30;
export const GLASS_OPACITY_MAX = 90;
export const DEFAULT_GLASS_OPACITY = 65;
export const DEFAULT_PROMINENT_ACTIVE_TAB = false;
export const NEUTRAL_ACCENT = "neutral";
const FOLLOW_SYSTEM_KEY = "buzz-follow-system";
const VIDEO_REVIEW_NEUTRAL_ACCENT = "0 0% 98%";
const VIDEO_REVIEW_CHIP_SURFACE = "#161616";
const VIDEO_REVIEW_TEXT_CONTRAST = 4.5;
const VIDEO_REVIEW_CHIP_BACKGROUND_ALPHAS = [0.15, 0.3] as const;
const GLASS_VIBRANCY_MATERIAL = "sidebar";

export const ACCENT_COLORS = [
  { name: "Neutral", value: NEUTRAL_ACCENT },
  { name: "Blue", value: "#3b82f6" },
  { name: "Cyan", value: "#06b6d4" },
  { name: "Green", value: "#22c55e" },
  { name: "Orange", value: "#f97316" },
  { name: "Red", value: "#ef4444" },
  { name: "Pink", value: "#ec4899" },
  { name: "Lilac", value: "#c0a2f1" },
  { name: "Purple", value: "#a855f7" },
  { name: "Indigo", value: "#6366f1" },
] as const;

const DEFAULT_ACCENT = CREW_ACCENT_HEX;

type ThemeContextValue = {
  themeName: ChromeThemeName;
  selectedThemeName: ChromeThemeName;
  syntaxThemeName: SyntaxThemeName;
  isDark: boolean;
  isLoading: boolean;
  accentColor: string;
  followSystem: boolean;
  glassBackground: boolean;
  glassOpacity: number;
  glassBackgroundSupported: boolean;
  prominentActiveTab: boolean;
  hasPair: boolean;
  terminalPalette: ThemeInfo["terminalPalette"] | null;
  setTheme: (name: string) => void;
  setSyntaxTheme: (name: string) => void;
  setAccentColor: (color: string) => void;
  setFollowSystem: (enabled: boolean) => void;
  applyAppearance: (appearance: {
    theme: string;
    syntax?: string;
    accent: string;
    followSystem: boolean;
  }) => void;
  setGlassBackground: (enabled: boolean) => void;
  setGlassOpacity: (opacity: number) => void;
  setProminentActiveTab: (enabled: boolean) => void;
};

type ThemeProviderProps = {
  children: ReactNode;
  defaultTheme?: ChromeThemeName;
};

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

function isValidSyntaxThemeName(name: string): name is SyntaxThemeName {
  return isShikiPaletteName(name);
}

/** Read stored chrome + syntax, migrating the old combined Shiki pref. */
function readStoredAppearance(fallback: ChromeThemeName): {
  chrome: ChromeThemeName;
  syntax: SyntaxThemeName;
  toastMessage: string | null;
} {
  const storedTheme = getStorageItem(THEME_STORAGE_KEY);
  const storedSyntax = getStorageItem(SYNTAX_STORAGE_KEY);
  const alreadyMigrated = getStorageItem(THEME_SPLIT_MIGRATION_KEY) === "true";
  const migrated = migrateStoredAppearance({
    storedTheme,
    storedSyntax,
    alreadyMigrated,
  });

  if (!alreadyMigrated) {
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, migrated.chrome);
      window.localStorage.setItem(SYNTAX_STORAGE_KEY, migrated.syntax);
      window.localStorage.setItem(THEME_SPLIT_MIGRATION_KEY, "true");
    } catch {
      // Keep the in-memory result even if storage is full.
    }
  }

  return {
    chrome: migrated.chrome || fallback,
    syntax: migrated.syntax,
    toastMessage: migrated.toastMessage,
  };
}

function getContrastColor(hex: string): string {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})/i.exec(hex);
  if (!m) return "#ffffff";
  const r = parseInt(m[1], 16);
  const g = parseInt(m[2], 16);
  const b = parseInt(m[3], 16);
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return lum > 0.5 ? "#000000" : "#ffffff";
}

type Rgb = {
  r: number;
  g: number;
  b: number;
};

function hexToRgb(hex: string): Rgb {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})/i.exec(hex);
  if (!m) return { r: 255, g: 255, b: 255 };
  return {
    r: parseInt(m[1], 16),
    g: parseInt(m[2], 16),
    b: parseInt(m[3], 16),
  };
}

function mixRgb(from: Rgb, to: Rgb, factor: number): Rgb {
  return {
    r: from.r + (to.r - from.r) * factor,
    g: from.g + (to.g - from.g) * factor,
    b: from.b + (to.b - from.b) * factor,
  };
}

function compositeRgb(foreground: Rgb, background: Rgb, alpha: number): Rgb {
  return mixRgb(background, foreground, alpha);
}

function relativeLuminance({ r, g, b }: Rgb): number {
  const [rs, gs, bs] = [r, g, b].map((channel) => {
    const value = channel / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
}

function contrastRatio(a: Rgb, b: Rgb): number {
  const aLum = relativeLuminance(a);
  const bLum = relativeLuminance(b);
  return (Math.max(aLum, bLum) + 0.05) / (Math.min(aLum, bLum) + 0.05);
}

function getReviewAccentForeground(hex: string): string {
  const accent = hexToRgb(hex);
  const surface = hexToRgb(VIDEO_REVIEW_CHIP_SURFACE);
  const white = { r: 255, g: 255, b: 255 };
  const backgrounds = VIDEO_REVIEW_CHIP_BACKGROUND_ALPHAS.map((alpha) =>
    compositeRgb(accent, surface, alpha),
  );
  let low = 0;
  let high = 1;

  for (let i = 0; i < 20; i++) {
    const mid = (low + high) / 2;
    const candidate = mixRgb(accent, white, mid);
    const minContrast = Math.min(
      ...backgrounds.map((background) => contrastRatio(candidate, background)),
    );

    if (minContrast >= VIDEO_REVIEW_TEXT_CONTRAST) {
      high = mid;
    } else {
      low = mid;
    }
  }

  return hexToHsl(rgbToHex(mixRgb(accent, white, high)));
}

function rgbToHex({ r, g, b }: Rgb): string {
  const clamp = (value: number) =>
    Math.max(0, Math.min(255, Math.round(value)));
  return `#${[r, g, b]
    .map((channel) => clamp(channel).toString(16).padStart(2, "0"))
    .join("")}`;
}

function applyAccentColor(value: string) {
  const root = document.documentElement;
  if (value === NEUTRAL_ACCENT) {
    const styles = window.getComputedStyle(root);
    const foreground = styles.getPropertyValue("--foreground").trim();
    const background = styles.getPropertyValue("--background").trim();
    root.style.setProperty("--buzz-selected-accent", foreground);
    root.style.setProperty(
      "--buzz-video-review-accent",
      VIDEO_REVIEW_NEUTRAL_ACCENT,
    );
    root.style.setProperty(
      "--buzz-video-review-accent-foreground",
      VIDEO_REVIEW_NEUTRAL_ACCENT,
    );
    root.style.setProperty("--primary", foreground);
    root.style.setProperty("--primary-foreground", background);
    root.style.setProperty("--sidebar-primary", foreground);
    root.style.setProperty("--sidebar-primary-foreground", background);
    root.style.setProperty("--sidebar-active", foreground);
    root.style.setProperty("--sidebar-active-foreground", background);
    return;
  }

  const hex = value;
  const accentHsl = hexToHsl(hex);
  const fgHsl = hexToHsl(getContrastColor(hex));
  root.style.setProperty("--buzz-selected-accent", accentHsl);
  root.style.setProperty("--buzz-video-review-accent", accentHsl);
  root.style.setProperty(
    "--buzz-video-review-accent-foreground",
    getReviewAccentForeground(hex),
  );
  root.style.setProperty("--primary", accentHsl);
  root.style.setProperty("--primary-foreground", fgHsl);
  root.style.setProperty("--sidebar-primary", accentHsl);
  root.style.setProperty("--sidebar-primary-foreground", fgHsl);
  root.style.setProperty("--sidebar-active", accentHsl);
  root.style.setProperty("--sidebar-active-foreground", fgHsl);
}

/**
 * The Buzz themes ship with a fixed neutral accent (the GitHub black/white
 * foreground) rather than a user-selectable accent color. When a Buzz theme is
 * active we force `NEUTRAL_ACCENT` regardless of the stored preference, and the
 * appearance panel hides the accent picker. The user's chosen accent is left
 * untouched in storage so it returns when they switch back to another theme.
 */
export function isBuzzTheme(themeName: string): boolean {
  return themeName === "buzz" || themeName === "buzz-dark";
}

/**
 * Resolve the accent to actually apply for a theme: Buzz themes are pinned to
 * the neutral accent; Crew chrome pins the one blue; every other leftover
 * path uses the stored/selected accent.
 */
function resolveEffectiveAccent(
  themeName: string,
  accentColor: string,
): string {
  if (isBuzzTheme(themeName)) return NEUTRAL_ACCENT;
  if (themeName === CREW_DARK_THEME_NAME) return CREW_ACCENT_HEX;
  if (themeName === "crew-light") return CREW_LIGHT_ACCENT_HEX;
  return accentColor;
}

/** Toggle the Buzz-specific gradient marker independently from glass. */
function applyBuzzSidebar(themeName: string) {
  const root = document.documentElement;
  if (isBuzzTheme(themeName)) {
    root.setAttribute("data-buzz-sidebar", "");
    // Keep the concrete Buzz variant on the root as well as the generic
    // marker. The gradient stylesheet matches this attribute directly, which
    // makes WKWebView invalidate the painted background when light/dark mode
    // changes instead of relying only on a custom-property dependency update.
    root.setAttribute("data-buzz-theme", themeName);
  } else {
    root.removeAttribute("data-buzz-sidebar");
    root.removeAttribute("data-buzz-theme");
  }
}

/**
 * Toggle the transparent CSS surfaces that reveal native macOS vibrancy behind
 * the navigation and outer chrome. The center content panel remains opaque.
 *
 * IMPORTANT: enabling glass exposes whatever the compositor paints
 * behind the webview. Only enable it once the native `NSVisualEffectView`
 * vibrancy layer and the active theme colors are both ready.
 */
function setGlassBackgroundActive(enabled: boolean) {
  const root = document.documentElement;
  if (enabled) {
    // WKWebView keeps its page canvas opaque unless the root background is an
    // inline transparent value, even when the equivalent author rule wins in
    // the stylesheet. Clear it before exposing the native vibrancy layer.
    root.style.setProperty("background", "transparent");
    root.setAttribute("data-glass-background", "");
  } else {
    root.removeAttribute("data-glass-background");
    root.style.removeProperty("background");
  }
}

/** Apply the optional higher-contrast selected navigation surface. */
function setProminentActiveTabActive(enabled: boolean) {
  document.documentElement.toggleAttribute(
    "data-prominent-active-tab",
    enabled,
  );
}

function clampGlassOpacity(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_GLASS_OPACITY;
  return Math.min(
    GLASS_OPACITY_MAX,
    Math.max(GLASS_OPACITY_MIN, Math.round(value)),
  );
}

function readStoredGlassOpacity(): number {
  const stored = getStorageItem(GLASS_OPACITY_STORAGE_KEY);
  return stored === null
    ? DEFAULT_GLASS_OPACITY
    : clampGlassOpacity(Number(stored));
}

/** Set the tint opacity layered above native blur; lower values reveal more. */
function applyGlassOpacity(value: number) {
  document.documentElement.style.setProperty(
    "--glass-background-opacity",
    `${clampGlassOpacity(value)}%`,
  );
}

/** Only the newest overlapping native glass request may update CSS state. */
let glassVibrancyRequest = 0;

/** Whether the native vibrancy layer is confirmed installed. */
let glassVibrancyReady = false;

/** The native layer does not need rebuilding when only the theme changes. */
let glassVibrancyEnabled = false;

/** Mirrors the current preference for the async theme/native handshake. */
let glassBackgroundPreferenceEnabled = false;

/** Theme colors must be installed before the transparent surface is exposed. */
let glassThemeReady = false;

/**
 * Theme loading and native vibrancy can finish in either order. Whichever lands
 * last reveals the glass once both prerequisites are ready.
 */
function maybeEnableGlassBackground(requestToken: number) {
  if (requestToken !== glassVibrancyRequest) return;
  if (!glassBackgroundPreferenceEnabled || !isMacPlatform()) return;
  if (!glassVibrancyReady || !glassThemeReady) return;
  setGlassBackgroundActive(true);
}

/**
 * Install native vibrancy before making the webview transparent. Non-macOS and
 * web builds retain the normal opaque theme surface.
 */
async function applyWindowGlass(enabled: boolean) {
  glassBackgroundPreferenceEnabled = enabled;
  const requestToken = ++glassVibrancyRequest;

  if (!isTauri() || !isMacPlatform()) {
    glassBackgroundPreferenceEnabled = false;
    glassVibrancyEnabled = false;
    glassVibrancyReady = false;
    setGlassBackgroundActive(false);
    return;
  }

  if (enabled && glassVibrancyEnabled && glassVibrancyReady) {
    maybeEnableGlassBackground(requestToken);
    return;
  }

  glassVibrancyReady = false;

  try {
    await invokeTauri<void>("set_window_vibrancy", {
      enabled,
      material: GLASS_VIBRANCY_MATERIAL,
    });
    if (requestToken !== glassVibrancyRequest) return;
    glassVibrancyEnabled = enabled;
    if (enabled && isMacPlatform()) {
      glassVibrancyReady = true;
      maybeEnableGlassBackground(requestToken);
    }
  } catch (error) {
    console.warn("set_window_vibrancy failed", error);
    if (requestToken !== glassVibrancyRequest) return;
    glassVibrancyEnabled = false;
    setGlassBackgroundActive(false);
  }
}

/** Apply cached CSS vars synchronously to prevent FOUC. */
function applyCachedVars(): string | null {
  try {
    const cached = readThemeCache();
    if (!cached) return null;
    const { themeName, vars, isDark } = cached;
    const root = document.documentElement;
    for (const [key, value] of Object.entries(vars)) {
      root.style.setProperty(key, value as string);
    }
    root.classList.remove("light", "dark");
    root.classList.add(isDark ? "dark" : "light");
    if (isChromeThemeName(themeName)) {
      root.setAttribute("data-crew-chrome", themeName);
      root.removeAttribute("data-buzz-sidebar");
      root.removeAttribute("data-buzz-theme");
    } else {
      applyBuzzSidebar(themeName);
    }
    glassThemeReady = true;

    const accent = getStorageItem(ACCENT_STORAGE_KEY) ?? DEFAULT_ACCENT;
    applyAccentColor(resolveEffectiveAccent(themeName, accent));

    return themeName;
  } catch {
    return null;
  }
}

/** The latest theme load is the only one allowed to write document styles. */
let themeApplyRequest = 0;

/** Apply Crew chrome tokens and load the syntax theme for the terminal. */
async function applyTheme(
  chrome: ChromeThemeName,
  syntax: SyntaxThemeName,
): Promise<{
  isDark: boolean;
  terminalPalette: ThemeInfo["terminalPalette"] | null;
} | null> {
  const requestToken = ++themeApplyRequest;
  const { isDark, vars } = applyCrewChromeDocument(chrome);
  glassThemeReady = true;
  maybeEnableGlassBackground(glassVibrancyRequest);
  writeThemeCache({
    themeName: chrome,
    syntaxThemeName: syntax,
    vars,
    isDark,
  });

  const terminalPalette = await loadSyntaxTerminalPalette(syntax);
  if (requestToken !== themeApplyRequest) return null;

  return { isDark, terminalPalette };
}

export function ThemeProvider({
  children,
  defaultTheme = DEFAULT_CHROME_THEME,
}: ThemeProviderProps) {
  const initialAppearanceRef = useRef<ReturnType<
    typeof readStoredAppearance
  > | null>(null);
  if (initialAppearanceRef.current === null) {
    applyCachedVars();
    initialAppearanceRef.current = readStoredAppearance(defaultTheme);
  }
  const initialAppearance = initialAppearanceRef.current;

  const [selectedTheme, setSelectedTheme] = useState<ChromeThemeName>(
    () => initialAppearance.chrome,
  );
  const [syntaxTheme, setSyntaxThemeState] = useState<SyntaxThemeName>(
    () => initialAppearance.syntax,
  );
  const [isDark, setIsDark] = useState<boolean>(() => {
    return document.documentElement.classList.contains("dark");
  });
  const [isLoading, setIsLoading] = useState(true);
  const [terminalPalette, setTerminalPalette] = useState<
    ThemeInfo["terminalPalette"] | null
  >(null);
  const loadingRef = useRef<string | null>(null);
  const [accentColor, setAccentColorState] = useState<string>(() => {
    return getStorageItem(ACCENT_STORAGE_KEY) ?? DEFAULT_ACCENT;
  });
  const [glassBackground, setGlassBackgroundState] = useState<boolean>(() => {
    const stored = getStorageItem(GLASS_BACKGROUND_STORAGE_KEY);
    const enabled = isTauri() && isMacPlatform() && stored === "true";
    glassBackgroundPreferenceEnabled = enabled;
    return enabled;
  });
  const [glassOpacity, setGlassOpacityState] = useState<number>(() => {
    const opacity = readStoredGlassOpacity();
    applyGlassOpacity(opacity);
    return opacity;
  });
  const [prominentActiveTab, setProminentActiveTabState] = useState<boolean>(
    () => {
      const stored = getStorageItem(PROMINENT_ACTIVE_TAB_STORAGE_KEY);
      return stored === null ? DEFAULT_PROMINENT_ACTIVE_TAB : stored === "true";
    },
  );
  const [followSystem, setFollowSystemState] = useState<boolean>(() => {
    const stored = getStorageItem(FOLLOW_SYSTEM_KEY);
    if (stored !== null) return stored === "true";
    // Fresh profiles default to Crew Dark (not System). Existing follow-system
    // prefs survive; the chrome pair is now Crew Light ↔ Crew Dark.
    return false;
  });
  const [systemIsDark, setSystemIsDark] = useState<boolean>(() => {
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  });

  const effectiveTheme: ChromeThemeName = followSystem
    ? resolveChromeSystemTheme(selectedTheme, systemIsDark)
    : selectedTheme;

  const hasPair = true;

  useEffect(() => {
    const thisTheme = `${effectiveTheme}:${syntaxTheme}`;
    loadingRef.current = thisTheme;
    setIsLoading(true);

    applyTheme(effectiveTheme, syntaxTheme).then((result) => {
      if (!result) return;
      if (loadingRef.current === thisTheme) {
        setIsDark(result.isDark);
        if (result.terminalPalette) {
          setTerminalPalette(result.terminalPalette);
        }
        setIsLoading(false);
      }
    });
  }, [effectiveTheme, syntaxTheme]);

  useEffect(() => {
    const appearance = initialAppearanceRef.current;
    const message = appearance?.toastMessage;
    if (!appearance || !message) return;
    toast.info(message, { duration: 8000 });
    initialAppearanceRef.current = { ...appearance, toastMessage: null };
  }, []);

  useEffect(() => {
    void applyWindowGlass(glassBackground);
  }, [glassBackground]);

  useEffect(() => {
    setProminentActiveTabActive(
      prominentActiveTab && isBuzzTheme(effectiveTheme),
    );
  }, [effectiveTheme, prominentActiveTab]);

  useEffect(() => {
    if (!followSystem) return;

    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handleMediaChange = (event: MediaQueryListEvent) => {
      setSystemIsDark(event.matches);
    };
    let disposed = false;
    let unlistenNativeTheme: (() => void) | undefined;

    setSystemIsDark(mq.matches);
    mq.addEventListener("change", handleMediaChange);

    if (isTauri()) {
      void getCurrentWindow()
        .onThemeChanged(({ payload }) => {
          if (!disposed) setSystemIsDark(payload === "dark");
        })
        .then((unlisten) => {
          if (disposed) {
            unlisten();
          } else {
            unlistenNativeTheme = unlisten;
          }
        })
        .catch((error) => {
          console.warn("system theme listener unavailable", error);
        });
    }

    return () => {
      disposed = true;
      mq.removeEventListener("change", handleMediaChange);
      unlistenNativeTheme?.();
    };
  }, [followSystem]);

  useEffect(() => {
    applyAccentColor(resolveEffectiveAccent(effectiveTheme, accentColor));
  }, [accentColor, effectiveTheme]);

  const setTheme = useCallback((name: string) => {
    const chrome = normalizeChromeThemeName(name);
    setSelectedTheme(chrome);
    window.localStorage.setItem(THEME_STORAGE_KEY, chrome);
  }, []);

  const setSyntaxTheme = useCallback((name: string) => {
    if (!isValidSyntaxThemeName(name)) return;
    setSyntaxThemeState(name);
    window.localStorage.setItem(SYNTAX_STORAGE_KEY, name);
  }, []);

  const setAccentColor = useCallback((color: string) => {
    window.localStorage.setItem(ACCENT_STORAGE_KEY, color);
    setAccentColorState(color);
  }, []);

  const setFollowSystem = useCallback((enabled: boolean) => {
    window.localStorage.setItem(FOLLOW_SYSTEM_KEY, enabled ? "true" : "false");
    setFollowSystemState(enabled);
  }, []);

  const applyAppearance = useCallback(
    (appearance: {
      theme: string;
      syntax?: string;
      accent: string;
      followSystem: boolean;
    }) => {
      const chrome = normalizeChromeThemeName(appearance.theme);
      const syntax =
        appearance.syntax && isValidSyntaxThemeName(appearance.syntax)
          ? appearance.syntax
          : isValidSyntaxThemeName(appearance.theme)
            ? appearance.theme
            : undefined;
      try {
        window.localStorage.setItem(THEME_STORAGE_KEY, chrome);
        window.localStorage.setItem(ACCENT_STORAGE_KEY, appearance.accent);
        window.localStorage.setItem(
          FOLLOW_SYSTEM_KEY,
          appearance.followSystem ? "true" : "false",
        );
        if (syntax) {
          window.localStorage.setItem(SYNTAX_STORAGE_KEY, syntax);
        }
      } catch {
        // Keep the active appearance responsive even if the local cache is full.
      }
      setSelectedTheme(chrome);
      if (syntax) setSyntaxThemeState(syntax);
      setAccentColorState(appearance.accent);
      setFollowSystemState(appearance.followSystem);
    },
    [],
  );

  const setGlassBackground = useCallback((enabled: boolean) => {
    if (!isTauri() || !isMacPlatform()) {
      glassBackgroundPreferenceEnabled = false;
      setGlassBackgroundActive(false);
      setGlassBackgroundState(false);
      return;
    }
    window.localStorage.setItem(
      GLASS_BACKGROUND_STORAGE_KEY,
      enabled ? "true" : "false",
    );
    glassBackgroundPreferenceEnabled = enabled;
    if (!enabled) {
      setGlassBackgroundActive(false);
    }
    setGlassBackgroundState(enabled);
  }, []);

  const setGlassOpacity = useCallback((opacity: number) => {
    const nextOpacity = clampGlassOpacity(opacity);
    window.localStorage.setItem(GLASS_OPACITY_STORAGE_KEY, String(nextOpacity));
    applyGlassOpacity(nextOpacity);
    setGlassOpacityState(nextOpacity);
  }, []);

  const setProminentActiveTab = useCallback((enabled: boolean) => {
    window.localStorage.setItem(
      PROMINENT_ACTIVE_TAB_STORAGE_KEY,
      enabled ? "true" : "false",
    );
    setProminentActiveTabState(enabled);
  }, []);

  const value: ThemeContextValue = {
    themeName: effectiveTheme,
    selectedThemeName: selectedTheme,
    syntaxThemeName: syntaxTheme,
    isDark,
    isLoading,
    accentColor,
    followSystem,
    glassBackground,
    glassOpacity,
    glassBackgroundSupported: isTauri() && isMacPlatform(),
    prominentActiveTab,
    hasPair,
    terminalPalette,
    setTheme,
    setSyntaxTheme,
    setAccentColor,
    setFollowSystem,
    applyAppearance,
    setGlassBackground,
    setGlassOpacity,
    setProminentActiveTab,
  };

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}
