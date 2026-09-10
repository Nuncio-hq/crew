import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";
import * as React from "react";
import * as jsx from "react/jsx-runtime";
import ts from "typescript";

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
afterEach(async () => (await import("@testing-library/react")).cleanup());
after(() => dom.window.close());

function loadTranscript(calls) {
  const exports = {};
  const dependencies = {
    react: React,
    "react/jsx-runtime": jsx,
    "@/features/agents/ui/AgentSessionTranscriptList": {
      AgentSessionTranscriptList: () =>
        React.createElement("p", null, "Transcript"),
    },
    "@/features/agents/ui/agentSessionTranscript": {
      buildTranscriptState: () => ({ items: [] }),
    },
    "@/features/agents/ui/useObserverEvents": {
      useArchivedChannelEvents: () => [],
      useLoadArchivedObserverEvents: () => {
        calls.loader += 1;
        throw new Error("transcript must not own archive paging");
      },
      useObserverEvents: () => ({ events: [] }),
    },
    "@/features/messages/lib/projectThreadMissionControl": {
      mergeProjectThreadPeekEvents: () => [],
    },
  };
  vmRun(
    fs.readFileSync(
      new URL("./ThreadAgentTranscript.tsx", import.meta.url),
      "utf8",
    ),
    exports,
    dependencies,
  );
  return exports.ThreadAgentTranscript;
}

function loadInformation(calls, sharedPaging) {
  const exports = {};
  const plans = [
    {
      agentName: "Worker A",
      agentPubkey: "a".repeat(64),
      entries: [],
      liveness: "idle",
      unknown: false,
    },
    {
      agentName: "Worker B",
      agentPubkey: "b".repeat(64),
      entries: [],
      liveness: "idle",
      unknown: false,
    },
  ];
  const dependencies = {
    react: React,
    "react/jsx-runtime": jsx,
    "@/features/messages/lib/threadForgeViewContextStore": {
      useThreadForgeViewContext: () => ({
        channelId: "channel",
        rootEventId: "root",
        messages: [{ id: "root", body: "Task" }],
        profiles: {},
      }),
    },
    "@/features/messages/ui/useDeclaredPlansForThread": {
      useDeclaredPlansForThread: () => ({
        archivePaging: sharedPaging,
        conversationId: "conversation",
        plans,
      }),
    },
    "@/features/messages/ui/DeclaredPlansRail": {
      DeclaredPlansRail: () => null,
    },
    "./ThreadActivityRunControls": {
      ThreadActivityRunControls: () => null,
    },
    "./ThreadAgentTranscript": {
      ThreadAgentTranscript: (props) => {
        calls.paging.push(props.archivePaging);
        return React.createElement("p", null, props.agent.agentName);
      },
    },
  };
  vmRun(
    fs.readFileSync(
      new URL("./ThreadInformationTab.tsx", import.meta.url),
      "utf8",
    ),
    exports,
    dependencies,
  );
  return exports.ThreadInformationTab;
}

function vmRun(source, exports, dependencies) {
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
      require: (key) => {
        assert.ok(key in dependencies, `unmocked dependency ${key}`);
        return dependencies[key];
      },
    },
  );
}

const agent = {
  agentName: "Worker",
  agentPubkey: "a".repeat(64),
  entries: [],
  liveness: "idle",
  unknown: false,
};
const paging = {
  fetchOlderArchived: async () => {},
  hasOlderArchived: true,
};

test("Activity transcript reuses caller paging across agent remounts", async () => {
  const { render, fireEvent } = await import("@testing-library/react");
  const calls = { loader: 0, fetch: 0 };
  const Transcript = loadTranscript(calls);
  const sharedPaging = {
    ...paging,
    fetchOlderArchived: async () => {
      calls.fetch += 1;
    },
  };
  const view = render(
    React.createElement(Transcript, {
      agent,
      archivePaging: sharedPaging,
      channelId: "channel",
      conversationId: "conversation",
    }),
  );
  assert.equal(calls.loader, 0);
  fireEvent.click(view.getByRole("button", { name: "Load older activity" }));
  assert.equal(calls.fetch, 1);

  view.rerender(
    React.createElement(Transcript, {
      agent: { ...agent, agentPubkey: "b".repeat(64) },
      archivePaging: sharedPaging,
      channelId: "channel",
      conversationId: "conversation",
    }),
  );
  assert.equal(calls.loader, 0);
  fireEvent.click(view.getByRole("button", { name: "Load older activity" }));
  assert.equal(calls.fetch, 2);
});

test("Activity agent switching forwards one channel paging owner", async () => {
  const { render, fireEvent } = await import("@testing-library/react");
  const calls = { paging: [] };
  const sharedPaging = { ...paging };
  const Information = loadInformation(calls, sharedPaging);
  const view = render(
    React.createElement(Information, {
      channelId: "channel",
      threadRootId: "root",
      tab: "activity",
    }),
  );
  assert.equal(calls.paging.length, 1);
  assert.equal(calls.paging[0], sharedPaging);
  fireEvent.change(view.getByRole("combobox", { name: "Activity agent" }), {
    target: { value: "b".repeat(64) },
  });
  assert.equal(calls.paging.length, 2);
  assert.equal(calls.paging[1], sharedPaging);
});
