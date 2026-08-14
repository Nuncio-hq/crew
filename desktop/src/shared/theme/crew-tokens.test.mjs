import assert from "node:assert/strict";
import test from "node:test";
import { CREW_DARK_HEX, CREW_LIGHT_HEX, crewTokenVars } from "./crew-tokens.ts";
import { CHROME_THEMES } from "./chrome-theme.ts";

function hexToRgb(hex) {
  const match = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  if (!match) throw new Error(`invalid hex ${hex}`);
  return {
    r: parseInt(match[1], 16),
    g: parseInt(match[2], 16),
    b: parseInt(match[3], 16),
  };
}

function linearChannel(channel) {
  const value = channel / 255;
  return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(hex) {
  const { r, g, b } = hexToRgb(hex);
  return (
    0.2126 * linearChannel(r) +
    0.7152 * linearChannel(g) +
    0.0722 * linearChannel(b)
  );
}

function contrastRatio(a, b) {
  const aLum = relativeLuminance(a);
  const bLum = relativeLuminance(b);
  return (Math.max(aLum, bLum) + 0.05) / (Math.min(aLum, bLum) + 0.05);
}

const AA_TEXT = 4.5;

const DARK_SURFACES = {
  shell: CREW_DARK_HEX.shell,
  content: CREW_DARK_HEX.content,
  raised: CREW_DARK_HEX.raised,
  overlay: CREW_DARK_HEX.overlay,
};

const LIGHT_SURFACES = {
  shell: CREW_LIGHT_HEX.shell,
  content: CREW_LIGHT_HEX.content,
  raised: CREW_LIGHT_HEX.raised,
  overlay: CREW_LIGHT_HEX.overlay,
};

function assertTextOnAllSurfaces(label, fg, surfaces) {
  for (const [surfaceName, bg] of Object.entries(surfaces)) {
    const ratio = contrastRatio(fg, bg);
    assert.ok(
      ratio >= AA_TEXT,
      `${label} on ${surfaceName} (${fg} / ${bg}) is ${ratio.toFixed(2)}:1, need ${AA_TEXT}`,
    );
  }
}

test("Crew Dark text tiers meet AA on all four surfaces", () => {
  assertTextOnAllSurfaces("primary", CREW_DARK_HEX.foreground, DARK_SURFACES);
  assertTextOnAllSurfaces("secondary", CREW_DARK_HEX.secondary, DARK_SURFACES);
  assertTextOnAllSurfaces("meta", CREW_DARK_HEX.meta, DARK_SURFACES);
});

test("Crew Light text tiers meet AA on all four surfaces", () => {
  assertTextOnAllSurfaces("primary", CREW_LIGHT_HEX.foreground, LIGHT_SURFACES);
  assertTextOnAllSurfaces(
    "secondary",
    CREW_LIGHT_HEX.secondary,
    LIGHT_SURFACES,
  );
  assertTextOnAllSurfaces("meta", CREW_LIGHT_HEX.meta, LIGHT_SURFACES);
});

test("Crew Dark semantic colors meet AA on all four surfaces", () => {
  for (const [name, hex] of [
    ["success", CREW_DARK_HEX.success],
    ["danger", CREW_DARK_HEX.danger],
    ["attention", CREW_DARK_HEX.attention],
    ["merged", CREW_DARK_HEX.merged],
  ]) {
    assertTextOnAllSurfaces(name, hex, DARK_SURFACES);
  }
});

test("Crew Light semantic colors meet AA on all four surfaces", () => {
  for (const [name, hex] of [
    ["success", CREW_LIGHT_HEX.success],
    ["danger", CREW_LIGHT_HEX.danger],
    ["attention", CREW_LIGHT_HEX.attention],
    ["merged", CREW_LIGHT_HEX.merged],
  ]) {
    assertTextOnAllSurfaces(name, hex, LIGHT_SURFACES);
  }
});

test("dark accent as a link meets AA on shell and content", () => {
  assertTextOnAllSurfaces("accent", CREW_DARK_HEX.accent, {
    shell: DARK_SURFACES.shell,
    content: DARK_SURFACES.content,
  });
});

test("filled primary buttons keep AA white labels on the light accent", () => {
  assert.ok(
    contrastRatio("#ffffff", CREW_LIGHT_HEX.accent) >= AA_TEXT,
    "white on Crew Light accent",
  );
});

test("both chrome themes expose the reserved semantic var keys", () => {
  const required = [
    "--background",
    "--card",
    "--popover",
    "--sidebar-background",
    "--foreground",
    "--muted-foreground",
    "--meta-foreground",
    "--primary",
    "--success",
    "--destructive",
    "--attention",
    "--merged",
    "--status-added",
    "--status-deleted",
    "--huddle-popover-surface",
    "--chart-1",
  ];
  for (const chrome of CHROME_THEMES) {
    const vars = crewTokenVars(chrome);
    for (const key of required) {
      assert.ok(vars[key], `${chrome} missing ${key}`);
    }
  }
});
