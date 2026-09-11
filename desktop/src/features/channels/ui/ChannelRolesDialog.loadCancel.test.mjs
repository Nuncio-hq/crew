import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

before(() => {
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
    MutationObserver: dom.window.MutationObserver,
    ResizeObserver: class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  });
  for (const key of Object.getOwnPropertyNames(dom.window)) {
    if (
      key in globalThis ||
      (!key.startsWith("HTML") &&
        !key.startsWith("SVG") &&
        !key.startsWith("CSS") &&
        ![
          "Node",
          "NodeFilter",
          "Event",
          "CustomEvent",
          "MouseEvent",
          "KeyboardEvent",
          "FocusEvent",
          "PointerEvent",
          "EventTarget",
          "Text",
          "Comment",
          "DocumentFragment",
          "Range",
          "Selection",
          "getComputedStyle",
        ].includes(key))
    )
      continue;
    if (dom.window[key] !== undefined) globalThis[key] = dom.window[key];
  }
  globalThis.getComputedStyle = dom.window.getComputedStyle.bind(dom.window);
  dom.window.requestAnimationFrame = (callback) => setTimeout(callback, 0);
  globalThis.requestAnimationFrame = dom.window.requestAnimationFrame;
  dom.window.matchMedia ??= () => ({
    matches: false,
    media: "",
    onchange: null,
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent: () => false,
  });
  globalThis.matchMedia = dom.window.matchMedia;
  const dispatch = dom.window.EventTarget.prototype.dispatchEvent;
  dom.window.EventTarget.prototype.dispatchEvent = function (event) {
    if (!(event instanceof dom.window.Event)) return false;
    return dispatch.call(this, event);
  };
});

after(() => dom.window.close());

function deferred() {
  let resolve;
  const promise = new Promise((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}

test("initial role loading keeps Cancel and close available and fences dismissal", async () => {
  const { default: React, act } = await import("react");
  const { render, screen, fireEvent, cleanup } = await import(
    "@testing-library/react"
  );
  const { ChannelRolesDialog } = await import("./ChannelRolesDialog.tsx");
  const { ThemeProvider } = await import("@/shared/theme/ThemeProvider.tsx");
  const pendingCanvas = deferred();
  const token = {
    scope: { owner: "a".repeat(64), community: "http://localhost" },
    workspace_generation: 1,
    identity_generation: 1,
  };
  let closed = 0;
  globalThis.__TAURI_INTERNALS__ = {
    invoke: async (command) => {
      if (command === "owner_operation_scope") return token;
      if (command === "get_canvas") return pendingCanvas.promise;
      if (command === "list_relay_agents") return [];
      if (command === "list_channel_crew_operations")
        return { token, value: [] };
      throw new Error(`unexpected command: ${command}`);
    },
  };
  window.__TAURI_INTERNALS__ = globalThis.__TAURI_INTERNALS__;

  function Surface() {
    const [open, setOpen] = React.useState(true);
    return open
      ? React.createElement(ChannelRolesDialog, {
          channelId: "channel",
          onClose: () => {
            closed += 1;
            setOpen(false);
          },
          onApplied: () => {},
        })
      : null;
  }

  try {
    render(
      React.createElement(
        ThemeProvider,
        { defaultTheme: "buzz" },
        React.createElement(Surface),
      ),
    );
    await act(async () => {});
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const close = screen.getByRole("button", { name: "Close" });
    assert.equal(cancel.disabled, false);
    assert.equal(close.disabled, false);

    await act(async () => {
      fireEvent.click(cancel);
    });
    assert.equal(closed, 1);
    assert.equal(screen.queryByTestId("channel-roles-dialog"), null);

    pendingCanvas.resolve({
      content: "# Late canvas",
      event_id: "late-head",
      definitions: [],
      stored_assignments: {},
      stored_routing: {},
      stored_capabilities: {},
      contact_pubkey: null,
      crew_authority: "owner",
      crew_parse_state: "valid",
      routing: [],
      assignments: [],
      dev_mcp_granted: null,
      crew_parse_error: null,
    });
    await act(async () => {});
  } finally {
    cleanup();
  }
});
