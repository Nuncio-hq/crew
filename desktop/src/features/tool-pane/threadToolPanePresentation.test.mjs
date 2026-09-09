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
before(() =>
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  }),
);
beforeEach(store.resetToolPaneForTests);
afterEach(async () => (await import("@testing-library/react")).cleanup());
after(() => dom.window.close());
const noop = () => null;
function loadPane() {
  const exports = {};
  const dependencies = {
    react: React,
    "react/jsx-runtime": jsx,
    "lucide-react": new Proxy({}, { get: () => noop }),
    "@/features/messages/ui/threadPrHub/ThreadPrHub": {
      ThreadPrHub: () => React.createElement("p", null, "PR details"),
    },
    "@/shared/lib/cn": { cn: (...values) => values.filter(Boolean).join(" ") },
    "./BrowserTab": { BrowserTab: noop },
    "./SimTab": { SimTab: noop },
    "./GovernorStrip": { GovernorStrip: noop },
    "./governorClient": {
      getCanvasTooling: async () => null,
      openToolPaneWindow: async () => {},
    },
    "./toolPaneStore": store,
    "./threadToolPaneSubject": { matchingThreadToolPaneSubject },
    "./ThreadInformationTab": {
      ThreadInformationTab: ({ tab }) =>
        React.createElement("p", null, `${tab} content`),
    },
  };
  vm.runInNewContext(
    ts.transpileModule(
      fs.readFileSync(
        new URL("./ChannelToolPane.tsx", import.meta.url),
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
      require: (key) => {
        assert.ok(key in dependencies, `unmocked dependency ${key}`);
        return dependencies[key];
      },
    },
  );
  return exports.ChannelToolPane;
}
const props = {
  channelId: "11111111-1111-1111-1111-111111111111",
  channelName: "Crew",
  threadRootId: "a".repeat(64),
  mode: "thread",
};
const subject = {
  kind: "pr",
  owner: "Nuncio-hq",
  name: "crew",
  number: 354,
  channelId: props.channelId,
  rootEventId: props.threadRootId,
  source: "url",
};

test("thread tools expose reachable Context, Activity and Agent plans tabs", async () => {
  const { render } = await import("@testing-library/react");
  const view = render(React.createElement(loadPane(), props));
  assert.ok(view.getByRole("tab", { name: "Context" }));
  assert.ok(view.getByRole("tab", { name: "Activity" }));
  assert.ok(view.getByRole("tab", { name: "Agent plans" }));
});

test("a PR subject cannot override Close and thread tools have no popout", async () => {
  const { render, fireEvent } = await import("@testing-library/react");
  store.openToolPane("pr");
  const view = render(
    React.createElement(loadPane(), { ...props, forgeSubject: subject }),
  );
  fireEvent.click(view.getByRole("button", { name: "Close Tools" }));
  assert.equal(store.getToolPaneSnapshot().open, false);
  assert.ok(
    view.queryByRole("button", { name: "Pop out Tools" }) === null,
    "thread popout must be absent",
  );
});

test("a URL PR subject for another thread is unavailable", async () => {
  const { render } = await import("@testing-library/react");
  store.openToolPane("pr");
  const view = render(
    React.createElement(loadPane(), {
      ...props,
      forgeSubject: { ...subject, rootEventId: "b".repeat(64) },
    }),
  );
  assert.ok(view.queryByText("PR details") === null, "wrong-root PR rendered");
});

test("tab keyboard navigation follows focus and ignores modified or composed keys", async () => {
  const { render, fireEvent } = await import("@testing-library/react");
  store.openToolPane("context");
  const view = render(React.createElement(loadPane(), props));
  const context = view.getByRole("tab", { name: "Context" });
  context.focus();
  fireEvent.keyDown(context, { key: "ArrowRight" });
  assert.equal(store.getToolPaneSnapshot().tab, "activity");
  assert.equal(
    document.activeElement,
    view.getByRole("tab", { name: "Activity" }),
  );
  fireEvent.keyDown(document.activeElement, { key: "End" });
  assert.equal(store.getToolPaneSnapshot().tab, "sim");
  fireEvent.keyDown(document.activeElement, { key: "Home" });
  assert.equal(store.getToolPaneSnapshot().tab, "context");
  for (const modifier of ["ctrlKey", "metaKey", "altKey", "isComposing"]) {
    fireEvent.keyDown(document.activeElement, {
      key: "ArrowRight",
      [modifier]: true,
    });
    assert.equal(store.getToolPaneSnapshot().tab, "context", modifier);
  }
});

test("a URL PR subject from another channel is unavailable", async () => {
  const { render } = await import("@testing-library/react");
  store.openToolPane("pr");
  const view = render(
    React.createElement(loadPane(), {
      ...props,
      forgeSubject: {
        ...subject,
        channelId: "22222222-2222-2222-2222-222222222222",
      },
    }),
  );
  assert.equal(view.queryByText("PR details"), null);
});
