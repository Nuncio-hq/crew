import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";
import * as React from "react";
import * as jsx from "react/jsx-runtime";
import ts from "typescript";
import { EMPTY_GOVERNOR_STATUS, VIEWPORT_PRESETS } from "./types.ts";

const dom = new JSDOM("<!doctype html><body></body>", {
  url: "http://localhost",
});
class ResizeObserver {
  observe() {}
  disconnect() {}
}
before(() =>
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    ResizeObserver,
    IS_REACT_ACT_ENVIRONMENT: true,
  }),
);
afterEach(async () => (await import("@testing-library/react")).cleanup());
after(() => dom.window.close());

// Run the actual components and effects. Stub native IPC/query boundaries only.
function loadInstrument(
  name,
  calls,
  status = EMPTY_GOVERNOR_STATUS,
  overrides = {},
) {
  const noop = () => null;
  const command =
    (name) =>
    async (...args) => {
      calls.push([name, ...args]);
    };
  const native = new Proxy(
    {
      getCanvasTooling: async () => null,
      holdingForChannel: (state, channelId) =>
        state.sims.find((sim) => sim.channelId === channelId),
      formatCountdown: () => "",
      formatBytes: () => "",
      ...overrides,
    },
    { get: (target, key) => (key in target ? target[key] : command(key)) },
  );
  const deps = {
    react: React,
    "react/jsx-runtime": jsx,
    sonner: {
      toast: { error: (message) => calls.push(["cleanup-error", message]) },
    },
    "lucide-react": new Proxy({}, { get: () => noop }),
    "@/features/community-members/hooks": {
      useMyRelayMembershipQuery: () => ({ data: { role: "owner" } }),
    },
    "@/features/terminal/terminalClient": { TerminalConnection: class {} },
    "@/shared/ui/button": {
      Button: ({ size, variant, ...props }) =>
        React.createElement("button", { type: "button", ...props }),
    },
    "@/shared/ui/input": {
      Input: (props) => React.createElement("input", props),
    },
    "@/shared/lib/cn": { cn: (...values) => values.filter(Boolean).join(" ") },
    "./governorClient": native,
    "./governorStore": {
      useGovernorStatus: () => status,
      invokeGovernor: command("invokeGovernor"),
    },
    "./postEvidenceCapture": {
      captureSimPng: command("capture"),
      postCaptureEvidence: command("post"),
    },
    "./agentControlStore": {
      isAgentControlChromeTarget: () => false,
      leaseFor: () => null,
      useAgentControlUi: () => ({}),
    },
    "./DrivingBanner": { DrivingBanner: noop },
    "./GhostCursorOverlay": { GhostCursorOverlay: noop },
    "./OriginApprovalCard": { PendingOriginPrompt: noop },
    "./types": { VIEWPORT_PRESETS },
  };
  function loadFile(file) {
    const source = fs.readFileSync(new URL(file, import.meta.url), "utf8");
    const exports = {};
    vm.runInNewContext(
      ts.transpileModule(source, {
        compilerOptions: {
          module: ts.ModuleKind.CommonJS,
          target: ts.ScriptTarget.ES2022,
          jsx: ts.JsxEmit.ReactJSX,
        },
      }).outputText,
      {
        exports,
        ResizeObserver,
        window: dom.window,
        HTMLElement: dom.window.HTMLElement,
        require: (key) => {
          if (key === "./threadNativeActivation")
            return loadFile("./threadNativeActivation.tsx");
          assert.ok(key in deps, `unmocked dependency ${key}`);
          return deps[key];
        },
      },
    );
    return exports;
  }
  return loadFile(`./${name}.tsx`)[name];
}
const props = {
  channelId: "channel-a",
  channelName: "Crew",
  threadRootId: "a".repeat(64),
  tooling: null,
};
const activationCalls = (calls) =>
  calls.filter(([name]) =>
    ["browserOpen", "simEnsureDevice", "simBoot", "startDevServer"].includes(
      name,
    ),
  );

test("selecting and reopening thread Browser issues no native activation", async () => {
  const { render } = await import("@testing-library/react");
  const calls = [];
  const Browser = loadInstrument("BrowserTab", calls);
  const first = render(React.createElement(Browser, props));
  first.unmount();
  render(React.createElement(Browser, props));
  assert.deepEqual(activationCalls(calls), []);
});

test("selecting thread Sim does not find or create a device", async () => {
  const { render } = await import("@testing-library/react");
  const calls = [];
  const Sim = loadInstrument("SimTab", calls);
  render(React.createElement(Sim, props));
  assert.deepEqual(activationCalls(calls), []);
});

test("explicit Browser activation opens Custom URL and closing still hides it", async () => {
  const { render, fireEvent } = await import("@testing-library/react");
  const calls = [];
  const Browser = loadInstrument("BrowserTab", calls);
  const view = render(React.createElement(Browser, props));
  fireEvent.click(view.getByRole("button", { name: "Activate Browser" }));
  assert.deepEqual(activationCalls(calls), [
    ["browserOpen", props.channelId, "http://127.0.0.1:5173"],
  ]);
  view.unmount();
  assert.equal(calls.filter(([name]) => name === "browserClose").length, 1);
});

test("thread navigation hides Browser without activating the next root", async () => {
  const { render, fireEvent } = await import("@testing-library/react");
  const calls = [];
  const Browser = loadInstrument("BrowserTab", calls);
  const view = render(React.createElement(Browser, props));
  fireEvent.click(view.getByRole("button", { name: "Activate Browser" }));
  view.rerender(
    React.createElement(Browser, { ...props, threadRootId: "b".repeat(64) }),
  );
  assert.equal(activationCalls(calls).length, 1);
  assert.equal(calls.filter(([name]) => name === "browserClose").length, 1);
  assert.ok(view.getByRole("button", { name: "Activate Browser" }));
  view.rerender(React.createElement(Browser, props));
  assert.equal(activationCalls(calls).length, 1);
  assert.ok(view.getByRole("button", { name: "Activate Browser" }));
});

test("existing simulator stays hidden until activation, then closes with visibility false", async () => {
  const { render, fireEvent } = await import("@testing-library/react");
  const calls = [];
  const Sim = loadInstrument("SimTab", calls, {
    ...EMPTY_GOVERNOR_STATUS,
    bridge: { ...EMPTY_GOVERNOR_STATUS.bridge, availability: "available" },
    sims: [{ channelId: props.channelId, lifecycle: "shutdown" }],
  });
  const view = render(React.createElement(Sim, props));
  assert.deepEqual(calls, []);
  fireEvent.click(view.getByRole("button", { name: "Activate Simulator" }));
  assert.deepEqual(
    calls.filter(([name]) => name === "simSetPaneVisible"),
    [["simSetPaneVisible", props.channelId, true]],
  );
  view.unmount();
  assert.deepEqual(
    calls.filter(([name]) => name === "simSetPaneVisible"),
    [
      ["simSetPaneVisible", props.channelId, true],
      ["simSetPaneVisible", props.channelId, false],
    ],
  );
  assert.deepEqual(activationCalls(calls), []);
});

test("channel Browser and Sim retain their existing mount behavior", async () => {
  const { render } = await import("@testing-library/react");
  const calls = [];
  const Browser = loadInstrument("BrowserTab", calls);
  const Sim = loadInstrument("SimTab", calls);
  render(React.createElement(Browser, { ...props, threadRootId: null }));
  render(React.createElement(Sim, { ...props, threadRootId: null }));
  assert.deepEqual(
    activationCalls(calls).map(([name]) => name),
    ["browserOpen", "simEnsureDevice"],
  );
});

function deferred() {
  let reject;
  const promise = new Promise((_, fail) => {
    reject = fail;
  });
  return { promise, reject };
}

test("old Browser bounds and close failures cannot disable a newer URL presentation", async () => {
  const { render, fireEvent, act } = await import("@testing-library/react");
  const calls = [],
    bounds = deferred(),
    close = deferred();
  let boundsCount = 0,
    closeCount = 0;
  const Browser = loadInstrument("BrowserTab", calls, EMPTY_GOVERNOR_STATUS, {
    setBrowserBounds: () =>
      ++boundsCount === 1 ? bounds.promise : Promise.resolve(),
    browserClose: () =>
      ++closeCount === 1 ? close.promise : Promise.resolve(),
  });
  const view = render(React.createElement(Browser, props));
  fireEvent.click(view.getByRole("button", { name: "Activate Browser" }));
  const url = view.getByDisplayValue("http://127.0.0.1:5173");
  fireEvent.change(url, { target: { value: "http://127.0.0.1:5174" } });
  fireEvent.keyDown(url, { key: "Enter" });
  await act(async () => bounds.reject(new Error("old bounds")));
  assert.ok(
    view.queryByRole("button", { name: "Activate Browser" }) === null,
    "old bounds disabled newer URL",
  );
  await act(async () => close.reject(new Error("old close")));
  assert.ok(
    view.queryByRole("button", { name: "Activate Browser" }) === null,
    "old close disabled newer URL",
  );
  assert.equal(view.queryByRole("alert"), null);
  assert.ok(
    calls.some(
      ([name, message]) =>
        name === "cleanup-error" && message.includes("old close"),
    ),
    "cleanup failure remains observable",
  );
});

test("current Browser failure surfaces, retry survives prior cleanup rejection", async () => {
  const { render, fireEvent, act } = await import("@testing-library/react");
  const calls = [],
    open = deferred(),
    close = deferred();
  let opens = 0,
    closes = 0;
  const Browser = loadInstrument("BrowserTab", calls, EMPTY_GOVERNOR_STATUS, {
    browserOpen: () => (++opens === 1 ? open.promise : Promise.resolve()),
    browserClose: () => (++closes === 1 ? close.promise : Promise.resolve()),
  });
  const view = render(React.createElement(Browser, props));
  fireEvent.click(view.getByRole("button", { name: "Activate Browser" }));
  await act(async () => open.reject(new Error("current open")));
  assert.match(view.getByRole("alert").textContent, /current open/);
  fireEvent.click(view.getByRole("button", { name: "Activate Browser" }));
  await act(async () => close.reject(new Error("previous close")));
  assert.ok(
    view.queryByRole("button", { name: "Activate Browser" }) === null,
    "old cleanup disabled retry",
  );
  assert.equal(view.queryByRole("alert"), null);
});

test("simulator scope return rejects old visibility errors but current failure remains recoverable", async () => {
  const { render, fireEvent, act } = await import("@testing-library/react");
  const calls = [],
    oldShow = deferred(),
    oldHide = deferred(),
    currentShow = deferred();
  let shows = 0,
    hides = 0;
  const Sim = loadInstrument(
    "SimTab",
    calls,
    {
      ...EMPTY_GOVERNOR_STATUS,
      bridge: { ...EMPTY_GOVERNOR_STATUS.bridge, availability: "available" },
      sims: [{ channelId: props.channelId, lifecycle: "shutdown" }],
    },
    {
      simSetPaneVisible: (_, visible) =>
        visible
          ? ++shows === 1
            ? oldShow.promise
            : shows === 2
              ? currentShow.promise
              : Promise.resolve()
          : ++hides === 1
            ? oldHide.promise
            : Promise.resolve(),
    },
  );
  const view = render(React.createElement(Sim, props));
  fireEvent.click(view.getByRole("button", { name: "Activate Simulator" }));
  view.rerender(
    React.createElement(Sim, { ...props, threadRootId: "b".repeat(64) }),
  );
  view.rerender(React.createElement(Sim, props));
  fireEvent.click(view.getByRole("button", { name: "Activate Simulator" }));
  await act(async () => oldShow.reject(new Error("old show")));
  await act(async () => oldHide.reject(new Error("old hide")));
  assert.ok(
    view.queryByRole("button", { name: "Activate Simulator" }) === null,
    "old visibility failure disabled new activation",
  );
  assert.equal(view.queryByRole("alert"), null);
  assert.ok(
    calls.some(
      ([name, message]) =>
        name === "cleanup-error" && message.includes("old hide"),
    ),
  );
  await act(async () => currentShow.reject(new Error("current show")));
  assert.match(view.getByRole("alert").textContent, /current show/);
  fireEvent.click(view.getByRole("button", { name: "Activate Simulator" }));
  assert.equal(view.queryByRole("alert"), null);
  assert.ok(
    view.queryByRole("button", { name: "Activate Simulator" }) === null,
  );
});
