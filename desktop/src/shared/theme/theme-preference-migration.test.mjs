import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_SYNTAX_THEME,
  formatSyntaxThemeLabel,
  isShikiPaletteName,
  migrateStoredAppearance,
  normalizeChromeThemeName,
} from "./theme-preference-migration.ts";

test("fresh profile stays Crew Dark + dark-plus with no toast", () => {
  const result = migrateStoredAppearance({
    storedTheme: null,
    storedSyntax: null,
    alreadyMigrated: false,
  });
  assert.deepEqual(result, {
    chrome: "crew-dark",
    syntax: "dark-plus",
    migratedFromLegacy: false,
    toastMessage: null,
  });
});

test("Shiki dark palette becomes Crew Dark chrome and keeps syntax", () => {
  const result = migrateStoredAppearance({
    storedTheme: "catppuccin-macchiato",
    storedSyntax: null,
    alreadyMigrated: false,
  });
  assert.equal(result.chrome, "crew-dark");
  assert.equal(result.syntax, "catppuccin-macchiato");
  assert.equal(result.migratedFromLegacy, true);
  assert.equal(
    result.toastMessage,
    "Your theme is now Crew Dark; code blocks kept Catppuccin Macchiato",
  );
});

test("Shiki light palette becomes Crew Light chrome and keeps syntax", () => {
  const result = migrateStoredAppearance({
    storedTheme: "catppuccin-latte",
    storedSyntax: null,
    alreadyMigrated: false,
  });
  assert.equal(result.chrome, "crew-light");
  assert.equal(result.syntax, "catppuccin-latte");
  assert.match(result.toastMessage ?? "", /Crew Light/);
});

test("buzz aliases map by luminance and do not keep a fake Shiki name", () => {
  const light = migrateStoredAppearance({
    storedTheme: "buzz",
    storedSyntax: null,
    alreadyMigrated: false,
  });
  assert.equal(light.chrome, "crew-light");
  assert.equal(light.syntax, DEFAULT_SYNTAX_THEME);
  assert.equal(isShikiPaletteName("buzz"), false);

  const dark = migrateStoredAppearance({
    storedTheme: "buzz-dark",
    storedSyntax: null,
    alreadyMigrated: false,
  });
  assert.equal(dark.chrome, "crew-dark");
  assert.equal(dark.syntax, DEFAULT_SYNTAX_THEME);
});

test("legacy light/dark/system strings map to Crew chrome", () => {
  assert.equal(
    migrateStoredAppearance({
      storedTheme: "light",
      storedSyntax: null,
      alreadyMigrated: false,
    }).chrome,
    "crew-light",
  );
  assert.equal(
    migrateStoredAppearance({
      storedTheme: "dark",
      storedSyntax: null,
      alreadyMigrated: false,
    }).chrome,
    "crew-dark",
  );
  assert.equal(
    migrateStoredAppearance({
      storedTheme: "system",
      storedSyntax: null,
      alreadyMigrated: false,
    }).chrome,
    "crew-dark",
  );
});

test("already-migrated Crew names are left alone", () => {
  const result = migrateStoredAppearance({
    storedTheme: "crew-light",
    storedSyntax: "dracula",
    alreadyMigrated: true,
  });
  assert.equal(result.chrome, "crew-light");
  assert.equal(result.syntax, "dracula");
  assert.equal(result.migratedFromLegacy, false);
  assert.equal(result.toastMessage, null);
});

test("houston is a dark Shiki palette", () => {
  const result = migrateStoredAppearance({
    storedTheme: "houston",
    storedSyntax: null,
    alreadyMigrated: false,
  });
  assert.equal(result.chrome, "crew-dark");
  assert.equal(result.syntax, "houston");
});

test("normalizeChromeThemeName accepts chrome and legacy names", () => {
  assert.equal(normalizeChromeThemeName("crew-dark"), "crew-dark");
  assert.equal(normalizeChromeThemeName("github-light"), "crew-light");
  assert.equal(normalizeChromeThemeName("dracula"), "crew-dark");
  assert.equal(normalizeChromeThemeName(null), "crew-dark");
});

test("formatSyntaxThemeLabel title-cases hyphenated names", () => {
  assert.equal(formatSyntaxThemeLabel("dark-plus"), "Dark Plus");
  assert.equal(
    formatSyntaxThemeLabel("catppuccin-macchiato"),
    "Catppuccin Macchiato",
  );
});
