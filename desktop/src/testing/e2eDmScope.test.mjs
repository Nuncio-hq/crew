import { register } from "node:module";
import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
  pretendToBeVisual: true,
});
Object.assign(globalThis, {
  window: dom.window,
  document: dom.window.document,
  localStorage: dom.window.localStorage,
  HTMLElement: dom.window.HTMLElement,
});
let invoke;
before(async () => {
  // The real mock bridge imports the transcript UI; Node has no Vite env.
  register(
    `data:text/javascript,${encodeURIComponent(`
    export async function load(url, context, nextLoad) {
      const loaded = await nextLoad(url, context);
      if (url.endsWith("/AgentSessionTranscriptList.tsx")) {
        return { ...loaded, source: String(loaded.source).replaceAll("import.meta.env", "({})") };
      }
      return loaded;
    }
  `)}`,
    import.meta.url,
  );
  window.__BUZZ_E2E__ = { mode: "mock", relayWsUrl: "wss://scope-a.example" };
  const { maybeInstallE2eTauriMocks } = await import("./e2eBridge.ts");
  maybeInstallE2eTauriMocks();
  invoke = window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
  assert.equal(typeof invoke, "function", "production mock handler installed");
});
after(() => dom.window.close());

test("mock open_dm enforces the native captured-relay assertion", async () => {
  await assert.rejects(
    invoke("open_dm", {
      pubkeys: ["a".repeat(64)],
      expectedRelayUrl: "wss://scope-b.example",
    }),
    /active community changed/,
  );
});
test("mock open_dm enforces the native captured-signer assertion", async () => {
  await assert.rejects(
    invoke("open_dm", {
      pubkeys: ["b".repeat(64)],
      expectedSignerPubkey: "0".repeat(64),
    }),
    /active identity changed/,
  );
});
