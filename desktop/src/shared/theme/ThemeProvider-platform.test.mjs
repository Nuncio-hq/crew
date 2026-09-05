import assert from "node:assert/strict";
import test from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { JSDOM } from "jsdom";
import {
  GLASS_BACKGROUND_STORAGE_KEY,
  ThemeProvider,
  useTheme,
} from "./ThemeProvider.tsx";

for (const scenario of [
  { platform: "Linux x86_64", native: true, supported: false },
  { platform: "Win32", native: true, supported: false },
  { platform: "MacIntel", native: false, supported: false },
  { platform: "MacIntel", native: true, supported: true },
]) {
  test(`stored glass preference respects ${scenario.platform}, native=${scenario.native}`, () => {
    const dom = new JSDOM("<!doctype html><html><body></body></html>", {
      url: "http://localhost",
    });
    const globals = [
      "window",
      "document",
      "navigator",
      "localStorage",
      "isTauri",
    ];
    const previous = globals.map((key) =>
      Object.getOwnPropertyDescriptor(globalThis, key),
    );
    Object.defineProperty(dom.window.navigator, "platform", {
      value: scenario.platform,
    });
    dom.window.matchMedia = () => ({ matches: false });
    for (const key of globals) {
      Object.defineProperty(globalThis, key, {
        configurable: true,
        writable: true,
        value: key === "isTauri" ? scenario.native : dom.window[key],
      });
    }
    try {
      localStorage.setItem(GLASS_BACKGROUND_STORAGE_KEY, "true");
      let theme;
      function Probe() {
        theme = useTheme();
        return null;
      }
      renderToStaticMarkup(
        React.createElement(ThemeProvider, null, React.createElement(Probe)),
      );
      assert.equal(theme.glassBackgroundSupported, scenario.supported);
      assert.equal(theme.glassBackground, scenario.supported);
      // Unsupported devices retain the stored preference for a supported host,
      // but trying to enable it cannot leave a transparent document surface.
      if (!scenario.supported) {
        document.documentElement.setAttribute("data-glass-background", "");
        theme.setGlassBackground(true);
        assert.equal(
          document.documentElement.hasAttribute("data-glass-background"),
          false,
        );
        assert.equal(
          localStorage.getItem(GLASS_BACKGROUND_STORAGE_KEY),
          "true",
        );
      }
    } finally {
      globals.forEach((key, index) => {
        if (previous[index])
          Object.defineProperty(globalThis, key, previous[index]);
        else delete globalThis[key];
      });
      dom.window.close();
    }
  });
}
