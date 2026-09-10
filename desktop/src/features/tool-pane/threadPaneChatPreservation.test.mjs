import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { after, afterEach, before, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import * as React from "react";
import * as jsx from "react/jsx-runtime";
import ts from "typescript";
import * as store from "./toolPaneStore.ts";
import { matchingThreadToolPaneSubject } from "./threadToolPaneSubject.ts";

const dom = new JSDOM("<!doctype html><body></body>", {
  url: "http://localhost",
});
class ResizeObserver {
  observe() {}
  disconnect() {}
}
const scope = {
  relayUrl: "wss://crew.example",
  viewerPubkey: "1".repeat(64),
  channelId: "11111111-1111-1111-1111-111111111111",
  rootEventId: "a".repeat(64),
};
before(() => {
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    Node: dom.window.Node,
    NodeFilter: dom.window.NodeFilter,
    HTMLInputElement: dom.window.HTMLInputElement,
    CustomEvent: dom.window.CustomEvent,
    MutationObserver: dom.window.MutationObserver,
    getComputedStyle: dom.window.getComputedStyle,
    ResizeObserver,
    IS_REACT_ACT_ENVIRONMENT: true,
  });
});
beforeEach(() => {
  store.resetToolPaneForTests();
  store.setToolPaneView(scope);
  dom.window.HTMLElement.prototype.getBoundingClientRect = () => ({
    width: 1400,
    height: 800,
    right: 1400,
  });
});
afterEach(async () => (await import("@testing-library/react")).cleanup());
after(() => dom.window.close());
function loadSplit(subject = null, dialog = null) {
  const exports = {};
  const deps = {
    react: React,
    "react/jsx-runtime": jsx,
    "@/features/tool-pane/ChannelToolPane": {
      ChannelToolPane: () =>
        React.createElement(
          "div",
          { "data-testid": "tools" },
          React.createElement(
            "button",
            { type: "button", onClick: store.closeToolPane },
            "Close Tools",
          ),
        ),
    },
    "@/features/tool-pane/toolPaneStore": store,
    "@/features/tool-pane/useThreadToolPaneScope": {
      useThreadToolPaneScope: () => scope,
    },
    "@/features/tool-pane/threadToolPaneSubject": {
      matchingThreadToolPaneSubject,
    },
    "@/shared/layout/auxiliaryPanelLayout": {
      clampAuxiliaryPanelWidth: (n) => n,
      getAuxiliaryPanelMaxWidth: (n) => n,
    },
    "@/shared/lib/cn": { cn: (...values) => values.filter(Boolean).join(" ") },
    "@/features/messages/lib/threadForgeHubSubjectStore": {
      useThreadForgeHubSubject: () => subject,
    },
    "./forgeHubCopy": { FORGE_HUB_NARROW_PX: 1000 },
    "@/shared/ui/dialog": dialog ?? {
      Dialog: ({ open, children }) => (open ? children : null),
      DialogContent: ({ children }) => children,
      DialogTitle: ({ children }) => React.createElement("h2", null, children),
    },
  };
  vm.runInNewContext(
    ts.transpileModule(
      fs.readFileSync(
        new URL(
          "../messages/ui/threadPrHub/ThreadFocusForgeSplit.tsx",
          import.meta.url,
        ),
        "utf8",
      ),
      {
        compilerOptions: {
          module: ts.ModuleKind.CommonJS,
          target: ts.ScriptTarget.ES2022,
          jsx: ts.JsxEmit.ReactJSX,
        },
      },
    ).outputText,
    {
      exports,
      window: dom.window,
      document: dom.window.document,
      HTMLElement: dom.window.HTMLElement,
      ResizeObserver,
      require: (key) => {
        assert.ok(key in deps, `unmocked dependency ${key}`);
        return deps[key];
      },
    },
  );
  return exports.ThreadFocusForgeSplit;
}
const props = {
  channelId: scope.channelId,
  channelName: "Crew",
  threadRootId: scope.rootEventId,
};

test("opening and closing tools preserves the mounted conversation and draft node", async () => {
  const { render, act } = await import("@testing-library/react");
  const view = render(
    React.createElement(
      loadSplit(),
      props,
      React.createElement("input", {
        "aria-label": "Draft",
        defaultValue: "Keep my draft",
      }),
    ),
  );
  const original = view.getByRole("textbox", { name: "Draft" });
  await act(async () => store.openToolPane());
  assert.ok(
    view.getByRole("textbox", { name: "Draft" }) === original,
    "conversation draft node was remounted",
  );
  await act(async () => store.closeToolPane());
  assert.ok(
    view.getByRole("textbox", { name: "Draft" }) === original,
    "conversation draft node was remounted",
  );
});

// Execute the real shared dialog and drawer, replacing styling/motion only.
async function loadNarrowDrawer(standalone = false) {
  const radix = await import("@radix-ui/react-dialog");
  function load(path, deps) {
    const exports = {};
    vm.runInNewContext(
      ts.transpileModule(
        fs.readFileSync(new URL(path, import.meta.url), "utf8"),
        {
          compilerOptions: {
            module: ts.ModuleKind.CommonJS,
            target: ts.ScriptTarget.ES2022,
            jsx: ts.JsxEmit.ReactJSX,
          },
        },
      ).outputText,
      {
        exports,
        window: dom.window,
        document: dom.window.document,
        HTMLElement: dom.window.HTMLElement,
        Node: dom.window.Node,
        ResizeObserver,
        requestAnimationFrame: (callback) => {
          callback(0);
          return 0;
        },
        require: (key) => {
          assert.ok(key in deps, `unmocked dependency ${key}`);
          return deps[key];
        },
      },
    );
    return exports;
  }
  const common = {
    react: React,
    "react/jsx-runtime": jsx,
    "@/shared/lib/cn": { cn: (...values) => values.filter(Boolean).join(" ") },
  };
  const dialog = load("../../shared/ui/dialog.tsx", {
    ...common,
    "@radix-ui/react-dialog": radix,
    "lucide-react": { X: () => null },
    "@/shared/ui/overlayCollision": { DIALOG_VIEWPORT_MAX_CLASS: "" },
    "@/shared/theme/ThemeProvider": { useTheme: () => ({ isDark: false }) },
    "./card-texture.css": {},
    "@/shared/ui/modalBackdrop": { MODAL_BACKDROP_BLUR_CLASS: "" },
    "@/shared/ui/modalMotion": {
      MODAL_CONTENT_MOTION_CLASS: "",
      MODAL_OVERLAY_MOTION_CLASS: "",
    },
  });
  const motion = Object.fromEntries(
    ["div", "button"].map((tag) => [
      tag,
      React.forwardRef(
        ({ animate, initial, exit, transition, ...props }, ref) =>
          React.createElement(tag, { ...props, ref }),
      ),
    ]),
  );
  const Drawer = load("../channels/ui/FocusThreadDrawer.tsx", {
    ...common,
    "motion/react": { motion, useReducedMotion: () => true },
    "@/features/channels/lib/threadFocusLayout": {
      THREAD_FOCUS_DRAWER_TRAVEL_PX: 0,
      THREAD_FOCUS_SLIVER_WIDTH_PX: 12,
    },
    "@/features/channels/lib/threadViewModePreference": {
      getThreadViewMode: () => "focus",
    },
    "@/features/messages/ui/threadPrHub/ThreadFocusForgeSplit": {
      ThreadFocusForgeSplit: loadSplit(null, dialog),
    },
    "@/shared/layout/auxiliaryPanelContext": {
      AuxiliaryPanelCloseOverrideContext: React.createContext(null),
    },
  }).FocusThreadDrawer;
  if (!standalone) return Drawer;
  return load("../channels/ui/ThreadPanelSurface.tsx", {
    ...common,
    "@/features/channels/ui/FocusThreadDrawer": { FocusThreadDrawer: Drawer },
    "@/features/channels/ui/useFocusDrawerPresence": {
      usePresenceCoverage: () => ({}),
    },
    "@/features/tool-pane/useThreadToolPaneScope": {
      useBindThreadToolPaneView: () => {},
    },
    "@/features/messages/ui/threadPrHub/ThreadFocusForgeSplit": {
      ThreadFocusForgeSplit: loadSplit(null, dialog),
    },
  }).ThreadPanelSurface;
}

test("narrow modal traps focus and Escape returns to the opener without closing chat", async (t) => {
  dom.window.HTMLElement.prototype.getBoundingClientRect = () => ({
    width: 800,
    height: 800,
    right: 800,
  });
  const { render, fireEvent, act } = await import("@testing-library/react");
  const Drawer = await loadNarrowDrawer();
  let closes = 0;
  const view = render(
    React.createElement(
      Drawer,
      {
        ...props,
        onClose: () => {
          closes++;
        },
      },
      React.createElement("input", {
        "aria-label": "Draft",
        defaultValue: "Unsaved draft",
      }),
      React.createElement(
        "button",
        {
          "aria-label": "Open thread tools",
          type: "button",
          onClick: () => store.openToolPane(),
        },
        "Tools",
      ),
    ),
  );
  const draft = view.getByRole("textbox", { name: "Draft" });
  const opener = view.getByRole("button", { name: "Open thread tools" });
  opener.focus();
  await act(async () => fireEvent.click(opener));
  const modal = view.getByRole("dialog", { name: "Thread tools" });
  assert.ok(draft.parentElement.hasAttribute("inert"));
  assert.ok(modal.contains(document.activeElement), "focus enters modal");
  await act(async () => draft.focus());
  assert.ok(
    modal.contains(document.activeElement),
    "focus cannot escape to covered chat",
  );
  await act(async () =>
    fireEvent.keyDown(document.activeElement, {
      key: "Escape",
      isComposing: true,
    }),
  );
  assert.equal(store.getToolPaneSnapshot().open, true);
  // Radix FocusScope defers its unmount autofocus with setTimeout(0).
  t.mock.timers.enable({ apis: ["setTimeout"] });
  await act(async () =>
    fireEvent.keyDown(document.activeElement, { key: "Escape" }),
  );
  await act(async () => t.mock.timers.tick(0));
  assert.equal(store.getToolPaneSnapshot().open, false);
  assert.equal(closes, 0, "one Escape must not also close the outer thread");
  assert.ok(view.getByRole("textbox", { name: "Draft" }) === draft);
  assert.equal(draft.value, "Unsaved draft");
  assert.ok(document.activeElement === opener, "focus returns to opener");
  await act(async () => fireEvent.keyDown(opener, { key: "Escape" }));
  assert.equal(closes, 1);
});

test("PR metadata alone cannot force tools open", async () => {
  const { render } = await import("@testing-library/react");
  const subject = {
    kind: "pr",
    channelId: scope.channelId,
    rootEventId: scope.rootEventId,
    source: "url",
  };
  const view = render(React.createElement(loadSplit(subject), props, "Chat"));
  assert.ok(
    view.queryByTestId("tools") === null,
    "PR metadata forced tools open",
  );
});

test("standalone thread hosts narrow tools without remounting or losing its draft", async (t) => {
  dom.window.HTMLElement.prototype.getBoundingClientRect = () => ({
    width: 800,
    height: 800,
    right: 800,
  });
  const { render, fireEvent, act } = await import("@testing-library/react");
  const Drawer = await loadNarrowDrawer(true);
  let closes = 0;
  const view = render(
    React.createElement(
      Drawer,
      {
        ...props,
        isStandalone: true,
        isFocusDrawer: false,
        covered: false,
        hasActiveEdit: false,
        onClose: () => {
          closes++;
        },
      },
      React.createElement("input", {
        "aria-label": "Draft",
        defaultValue: "Unsaved draft",
      }),
      React.createElement(
        "button",
        {
          "aria-label": "Open thread tools",
          type: "button",
          onClick: () => store.openToolPane(),
        },
        "Tools",
      ),
    ),
  );
  const draft = view.getByRole("textbox", { name: "Draft" });
  const opener = view.getByRole("button", { name: "Open thread tools" });
  opener.focus();
  await act(async () => fireEvent.click(opener));
  const modal = view.getByRole("dialog", { name: "Thread tools" });
  assert.ok(draft.parentElement.hasAttribute("inert"));
  assert.ok(modal.contains(document.activeElement), "focus enters modal");
  await act(async () => draft.focus());
  assert.ok(
    modal.contains(document.activeElement),
    "focus cannot escape to covered chat",
  );
  await act(async () =>
    fireEvent.keyDown(document.activeElement, {
      key: "Escape",
      isComposing: true,
    }),
  );
  assert.equal(store.getToolPaneSnapshot().open, true);
  // Radix FocusScope defers its unmount autofocus with setTimeout(0).
  t.mock.timers.enable({ apis: ["setTimeout"] });
  await act(async () =>
    fireEvent.keyDown(document.activeElement, { key: "Escape" }),
  );
  await act(async () => t.mock.timers.tick(0));
  assert.equal(store.getToolPaneSnapshot().open, false);
  assert.equal(closes, 0, "one Escape must not also close the outer thread");
  assert.ok(view.getByRole("textbox", { name: "Draft" }) === draft);
  assert.equal(draft.value, "Unsaved draft");
  assert.ok(document.activeElement === opener, "focus returns to opener");
});
