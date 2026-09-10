import assert from "node:assert/strict";
import { after, afterEach, before, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import { useToolPaneShortcuts } from "./useToolPaneShortcuts.ts";
import {
  resetThreadForgeViewContext,
  setThreadForgeViewContext,
} from "@/features/messages/lib/threadForgeViewContextStore.ts";
import {
  getThreadViewMode,
  setThreadViewMode,
} from "@/features/channels/lib/threadViewModePreference.ts";
import {
  getToolPaneSnapshot,
  openToolPane,
  resetToolPaneForTests,
} from "./toolPaneStore.ts";

const dom = new JSDOM("<!doctype html><body></body>", {
  url: "http://localhost",
});
before(() =>
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  }),
);
beforeEach(() => {
  resetToolPaneForTests();
  resetThreadForgeViewContext();
  setThreadViewMode("split");
});
afterEach(async () => (await import("@testing-library/react")).cleanup());
after(() => {
  resetThreadForgeViewContext();
  dom.window.close();
});
function key(options, target = window) {
  const event = new dom.window.KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    ...options,
  });
  target.dispatchEvent(event);
  return event;
}

test("no valid channel means shortcuts cannot open a resource pane", async () => {
  const { renderHook } = await import("@testing-library/react");
  renderHook(() => useToolPaneShortcuts(null));
  key({ code: "KeyB", metaKey: true, shiftKey: true });
  assert.equal(getToolPaneSnapshot().open, false);
});

test("channel Browser shortcut preserves modifier and composition ownership", async () => {
  const { renderHook } = await import("@testing-library/react");
  const view = renderHook(() => useToolPaneShortcuts("channel"));
  key({ code: "KeyB", metaKey: true });
  assert.equal(getToolPaneSnapshot().open, false);
  key({ code: "KeyB", metaKey: true, shiftKey: true, altKey: true });
  assert.equal(getToolPaneSnapshot().open, false);
  key({ code: "KeyB", metaKey: true, shiftKey: true, isComposing: true });
  assert.equal(getToolPaneSnapshot().open, false);
  key({ code: "KeyB", metaKey: true, shiftKey: true });
  assert.equal(getToolPaneSnapshot().tab, "browser");
  assert.equal(getToolPaneSnapshot().open, true);
  view.unmount();
  resetToolPaneForTests();
  key({ code: "KeyB", metaKey: true, shiftKey: true });
  assert.equal(getToolPaneSnapshot().open, false);
});

test("thread Browser and Sim shortcuts enter the focus tool host from split mode", async () => {
  const { renderHook } = await import("@testing-library/react");
  renderHook(() => useToolPaneShortcuts("channel"));
  setThreadForgeViewContext({
    channelId: "channel",
    rootEventId: "root",
    messages: [],
  });

  key({ code: "KeyB", metaKey: true, shiftKey: true });
  assert.equal(getThreadViewMode(), "focus");
  assert.deepEqual(getToolPaneSnapshot(), {
    open: true,
    tab: "browser",
    poppedOut: false,
  });

  resetToolPaneForTests();
  setThreadViewMode("split");
  key({ code: "KeyM", ctrlKey: true, shiftKey: true });
  assert.equal(getThreadViewMode(), "focus");
  assert.deepEqual(getToolPaneSnapshot(), {
    open: true,
    tab: "sim",
    poppedOut: false,
  });
});

test("Escape claimed by a modal does not reach the global pane handler", async () => {
  const { renderHook } = await import("@testing-library/react");
  renderHook(() => useToolPaneShortcuts("channel"));
  openToolPane();
  const event = new dom.window.KeyboardEvent("keydown", {
    key: "Escape",
    cancelable: true,
  });
  event.preventDefault();
  window.dispatchEvent(event);
  assert.equal(getToolPaneSnapshot().open, true);
});
