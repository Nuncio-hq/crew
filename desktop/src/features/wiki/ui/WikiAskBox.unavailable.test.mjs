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

const { cleanup, fireEvent, render, screen } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { WikiAskBox } = await import("@/features/wiki/ui/WikiAskBox");
const { WIKI_ASK_UNAVAILABLE_REASON } = await import(
  "@/features/wiki/lib/wikiAsk"
);

beforeEach(() => {
  cleanup();
  dom.window.localStorage.clear();
});

after(() => {
  cleanup();
  dom.window.close();
});

function mount() {
  return render(
    React.createElement(WikiAskBox, {
      channelId: "channel-1",
      door: "library",
      owner: "a".repeat(64),
      repoD: "crew",
      scopeLabel: "Asking about Crew",
    }),
  );
}

test("keeps the Ask composer visible while truthfully unavailable", () => {
  mount();

  assert.ok(screen.getByTestId("wiki-ask"));
  assert.equal(
    screen.getByText("Asking about Crew").textContent,
    "Asking about Crew",
  );
  assert.ok(screen.getByTestId("wiki-ask-mode"));
  assert.ok(screen.getByTestId("wiki-ask-input"));

  const status = screen.getByTestId("wiki-ask-unavailable");
  assert.equal(status.getAttribute("role"), "status");
  assert.equal(status.textContent, WIKI_ASK_UNAVAILABLE_REASON);

  const askButton = screen.getByRole("button", { name: "Ask" });
  assert.equal(askButton.disabled, true);
  assert.equal(
    askButton.getAttribute("aria-describedby"),
    "wiki-ask-unavailable",
  );
});

test("pointer, keyboard, and IME submit paths never answer or mutate a draft", () => {
  const existingDraftKey = "buzz-drafts.v2:https://relay.example:viewer";
  const existingDraft = JSON.stringify({
    "channel-1": {
      channelId: "channel-1",
      content: "existing channel draft",
      selectionEnd: 21,
      selectionStart: 21,
    },
  });
  dom.window.localStorage.setItem(existingDraftKey, existingDraft);
  mount();

  const input = screen.getByTestId("wiki-ask-input");
  const askButton = screen.getByRole("button", { name: "Ask" });
  const beforeDraft = dom.window.localStorage.getItem(existingDraftKey);
  const originalSetItem = dom.window.localStorage.setItem;
  let writes = 0;
  dom.window.localStorage.setItem = (...args) => {
    writes += 1;
    return originalSetItem.apply(dom.window.localStorage, args);
  };

  fireEvent.change(input, { target: { value: "How should this work?" } });
  fireEvent.click(askButton);
  const regularEnter = new dom.window.KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    code: "Enter",
    isComposing: false,
    key: "Enter",
  });
  input.dispatchEvent(regularEnter);
  assert.equal(regularEnter.defaultPrevented, true);
  fireEvent.compositionStart(input);
  const composingEnter = new dom.window.KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    code: "Enter",
    isComposing: true,
    key: "Enter",
  });
  input.dispatchEvent(composingEnter);
  assert.equal(composingEnter.defaultPrevented, false);
  fireEvent.compositionEnd(input);

  assert.equal(input.value, "How should this work?");
  assert.equal(writes, 0);
  assert.equal(dom.window.localStorage.getItem(existingDraftKey), beforeDraft);
  assert.equal(screen.queryByTestId("wiki-ask-answer"), null);
  assert.equal(screen.queryByTestId("wiki-start-thread"), null);
});

test("mode and question remain editable without enabling an unverified Ask", () => {
  mount();

  const mode = screen.getByTestId("wiki-ask-mode");
  const input = screen.getByTestId("wiki-ask-input");
  const askButton = screen.getByRole("button", { name: "Ask" });

  fireEvent.change(mode, { target: { value: "plan" } });
  fireEvent.change(input, { target: { value: "Plan the migration" } });

  assert.equal(mode.value, "plan");
  assert.equal(input.value, "Plan the migration");
  assert.equal(askButton.disabled, true);
  assert.equal(
    screen.getByTestId("wiki-ask-unavailable").textContent,
    WIKI_ASK_UNAVAILABLE_REASON,
  );
});
