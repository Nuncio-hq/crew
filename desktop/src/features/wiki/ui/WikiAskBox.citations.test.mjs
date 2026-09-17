/**
 * The developer composer's answer surface: citations as controls, and the
 * honest note when an attempt could not be kept on this machine.
 *
 * These drive the real component through the real Tauri invoke boundary (a
 * stubbed `__TAURI_INTERNALS__`, which is what `@tauri-apps/api` calls), so
 * removing `onOpenSource` from `CitationLink`, or the `historyRecorded` branch
 * from the composer, fails here.
 */
import assert from "node:assert/strict";
import { after, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

class NoopObserver {
  disconnect() {}
  observe() {}
  unobserve() {}
}

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  localStorage: dom.window.localStorage,
  window: dom.window,
  ResizeObserver: NoopObserver,
  IntersectionObserver: NoopObserver,
});
if (!dom.window.crypto?.randomUUID) {
  dom.window.crypto = globalThis.crypto;
}

const CITATION = {
  path: "src/answer.rs",
  startLine: 10,
  endLine: 12,
};

/** Every run the stub served, so a test can inspect what the UI sent. */
let runCalls = [];
let cancelCalls = [];
let runResult = null;
let runError = null;

dom.window.__TAURI_INTERNALS__ = {
  transformCallback: (callback) => callback,
  invoke: async (command, payload) => {
    switch (command) {
      case "private_ask_dev_status":
        return { enabled: true, blockedReason: null };
      case "private_ask_history":
        return [];
      case "private_ask_run":
        runCalls.push(payload);
        if (runError) throw new Error(runError);
        return { ...runResult, attemptId: payload.attemptId };
      case "private_ask_cancel":
        cancelCalls.push(payload);
        return null;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  },
};

const { act, cleanup, fireEvent, render, screen } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { WikiAskBox } = await import("@/features/wiki/ui/WikiAskBox");

beforeEach(() => {
  cleanup();
  runCalls = [];
  cancelCalls = [];
  runError = null;
  runResult = {
    markdown: "The answer.",
    refusal: null,
    citations: [CITATION],
    historyRecorded: true,
  };
});

after(() => {
  cleanup();
  dom.window.close();
});

async function mountAndAsk(onOpenSource) {
  const view = render(
    React.createElement(WikiAskBox, {
      channelId: "channel-1",
      door: "project",
      onOpenSource,
      owner: "a".repeat(64),
      repoD: "crew",
      scopeLabel: "Asking about Crew",
    }),
  );
  // The status poll decides which composer renders.
  await act(async () => {});
  fireEvent.change(screen.getByTestId("wiki-ask-dev-input"), {
    target: { value: "What does answer do?" },
  });
  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-ask-dev-submit"));
  });
  return view;
}

test("a citation opens the cited source when the page supplied an opener", async () => {
  const opened = [];
  await mountAndAsk((citation) => opened.push(citation));

  assert.equal(
    screen
      .getByTestId("wiki-ask-dev-answer")
      .textContent.includes("The answer."),
    true,
  );
  const citation = screen.getByTestId(`wiki-ask-dev-citation-${CITATION.path}`);
  assert.equal(citation.tagName, "BUTTON");
  fireEvent.click(citation);
  assert.deepEqual(opened, [CITATION]);

  // The run carried an id the surface minted, which is what makes Cancel
  // able to name it.
  assert.equal(runCalls.length, 1);
  assert.match(
    runCalls[0].attemptId,
    /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
  );
});

test("a citation with no opener stays text rather than a control going nowhere", async () => {
  await mountAndAsk(undefined);
  const citation = screen.getByTestId(`wiki-ask-dev-citation-${CITATION.path}`);
  assert.equal(citation.tagName, "SPAN");
});

test("an answer that could not be kept says so", async () => {
  runResult = { ...runResult, historyRecorded: false };
  await mountAndAsk(undefined);
  assert.ok(screen.getByTestId("wiki-ask-dev-history-unrecorded"));
});

test("a refusal that could not be kept says so too", async () => {
  // The refused path used to drop this: the backend recorded it with
  // `let _ = record(...)` and the refusal came back as a bare error string.
  runResult = {
    markdown: "",
    refusal: "selected agent is unavailable",
    citations: [],
    historyRecorded: false,
  };
  await mountAndAsk(undefined);
  assert.equal(
    screen.getByTestId("wiki-ask-dev-error").textContent,
    "selected agent is unavailable",
  );
  assert.ok(screen.getByTestId("wiki-ask-dev-history-unrecorded"));
});

test("a command that could not run at all is still shown as a refusal", async () => {
  runError = "private Ask is not enabled in this build";
  await mountAndAsk(undefined);
  assert.equal(
    screen.getByTestId("wiki-ask-dev-error").textContent,
    "private Ask is not enabled in this build",
  );
  // Nothing was attempted, so nothing is claimed about the machine's record.
  assert.equal(screen.queryByTestId("wiki-ask-dev-history-unrecorded"), null);
});
