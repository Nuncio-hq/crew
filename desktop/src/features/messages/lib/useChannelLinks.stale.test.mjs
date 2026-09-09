import assert from "node:assert/strict";
import { after, afterEach, beforeEach, mock, test } from "node:test";
import { createElement } from "react";
import { JSDOM } from "jsdom";

import { ChannelNavigationProvider } from "@/shared/context/ChannelNavigationContext";
import { useChannelLinks } from "./useChannelLinks.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});
const { act, cleanup, renderHook } = await import("@testing-library/react");
const channels = ["general", "engineering"].map((name) => ({
  id: name,
  name,
  channelType: "stream",
  archivedAt: null,
}));
const original = "Welcome to #general";
const replacement = "Edited, not deleted";

beforeEach(() => mock.timers.enable({ apis: ["setTimeout"] }));
afterEach(() => {
  cleanup();
  mock.timers.reset();
});
after(() => dom.window.close());

function mount() {
  const hook = renderHook(useChannelLinks, {
    wrapper: ({ children }) =>
      createElement(ChannelNavigationProvider, { channels }, children),
  });
  act(() => hook.result.current.updateChannelQuery(original, original.length));
  act(() => mock.timers.tick(120));
  assert.equal(hook.result.current.isChannelOpen, true);
  assert.equal(hook.result.current.channelSuggestions[0].name, "general");
  return hook;
}

function keyEvent(key, modifiers = {}) {
  return {
    key,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    ...modifiers,
  };
}

for (const key of ["Enter", "Tab"]) {
  test(`replacement invalidates #channel before debounce and releases ${key}`, () => {
    const { result } = mount();
    act(() =>
      result.current.updateChannelQuery(replacement, replacement.length),
    );
    // No time elapses between the replacement and submission/navigation.
    assert.equal(result.current.isChannelOpen, false);
    const event = keyEvent(key);
    assert.deepEqual(result.current.handleChannelKeyDown(event), {
      handled: false,
    });
    assert.equal(event.defaultPrevented, false);
  });

  test(`already-rendered ${key} callback rejects a replaced query before rerender`, () => {
    const { result } = mount();
    const published = result.current;
    const event = keyEvent(key);
    act(() => {
      published.updateChannelQuery(replacement, replacement.length);
      assert.deepEqual(published.handleChannelKeyDown(event), {
        handled: false,
      });
    });
    assert.equal(event.defaultPrevented, false);
  });

  test(`valid ${key} completion still inserts the selected channel`, () => {
    const { result } = mount();
    const event = keyEvent(key);
    const selected = result.current.handleChannelKeyDown(event);
    assert.equal(selected.handled, true);
    assert.equal(event.defaultPrevented, true);
    let edit;
    act(() => {
      edit = result.current.insertChannel(selected.suggestion, original.length);
    });
    assert.deepEqual(edit, {
      replaceFromOffset: 11,
      replaceToOffset: original.length,
      insertText: "#general ",
    });
    assert.equal(result.current.isChannelOpen, false);
  });
}

for (const [label, text, caret] of [
  ["replacement", replacement, replacement.length],
  ["caret moved outside the query", original, 0],
  ["query moved to another offset", "#general", 8],
  ["query changed to another channel", "Welcome to #eng", 15],
]) {
  test(`pending pointer selection cannot edit after ${label}`, () => {
    const { result } = mount();
    const published = result.current;
    const suggestion = published.channelSuggestions[0];
    let edit;
    act(() => {
      published.updateChannelQuery(text, caret);
      // Pointer handlers can still hold the prior rendered suggestion.
      edit = published.insertChannel(suggestion, caret);
    });
    assert.equal(edit, null);
    assert.equal(result.current.isChannelOpen, false);
  });
}

test("caret movement closes the overlay before its old query timer can run", () => {
  const { result } = mount();
  act(() => result.current.updateChannelQuery(original, 0));
  assert.equal(result.current.isChannelOpen, false);
  act(() => mock.timers.tick(120));
  assert.equal(result.current.isChannelOpen, false);
});

test("a fresh query offers valid pointer completion after debounce", () => {
  const { result } = mount();
  const text = "Ask #eng";
  act(() => result.current.updateChannelQuery(text, text.length));
  assert.equal(result.current.isChannelOpen, false);
  act(() => mock.timers.tick(120));
  assert.equal(result.current.channelSuggestions[0].name, "engineering");
  let edit;
  act(() => {
    edit = result.current.insertChannel(
      result.current.channelSuggestions[0],
      text.length,
    );
  });
  assert.deepEqual(edit, {
    replaceFromOffset: 4,
    replaceToOffset: text.length,
    insertText: "#engineering ",
  });
});

for (const [key, modifiers] of [
  ["Tab", { shiftKey: true }],
  ["Enter", { shiftKey: true }],
  ["Enter", { ctrlKey: true }],
  ["Enter", { metaKey: true }],
  ["Enter", { altKey: true }],
]) {
  test(`${Object.keys(modifiers)[0]}+${key} remains unclaimed`, () => {
    const { result } = mount();
    const event = keyEvent(key, modifiers);
    assert.deepEqual(result.current.handleChannelKeyDown(event), {
      handled: false,
    });
    assert.equal(event.defaultPrevented, false);
    assert.equal(result.current.isChannelOpen, true);
  });
}
