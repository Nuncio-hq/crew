import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { before, after, afterEach, test } from "node:test";
import * as React from "react";
import { JSDOM } from "jsdom";
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
const selection = Object.freeze({
  relayUrl: "wss://crew.example",
  viewerPubkey: "a".repeat(64),
  channelId: "channel",
  rootEventId: "root",
  conversationId: "thread",
  agentPubkey: "b".repeat(64),
  sessionId: "session",
  turnId: "turn",
});
function harness() {
  const listeners = new Set(),
    results = new Map();
  const state = {
    viewer: selection.viewerPubkey,
    relay: selection.relayUrl,
    root: selection.rootEventId,
    owned: new Set([selection.agentPubkey]),
    turns: [{ ...selection }],
    sends: [],
  };
  const deps = {
    react: React,
    "@/shared/api/hooks": {
      useIdentityQuery: () => ({ data: { pubkey: state.viewer } }),
    },
    "@/features/communities/useCommunities": {
      useCommunities: () => ({ activeCommunity: { relayUrl: state.relay } }),
    },
    "@/features/home/useOwnedAgentPubkeys": {
      useCurrentOwnedAgentPubkeys: () => state.owned,
    },
    "@/shared/lib/normalizeRelayUrl": { normalizeRelayUrl: (x) => x },
    "@/features/agents/activeAgentTurnsStore": {
      walkActiveAgentTurns: (fn) =>
        state.turns.forEach((t) => {
          fn(t.agentPubkey, t, 0);
        }),
      subscribeActiveAgentTurns: (fn) => {
        listeners.add(fn);
        return () => listeners.delete(fn);
      },
    },
    "@/features/agents/controlResultDispatch": {
      subscribeControlResults: (agent, fn) => {
        results.set(agent, fn);
        return () => results.delete(agent);
      },
    },
  };
  function load(path) {
    const source = ts.transpileModule(fs.readFileSync(path, "utf8"), {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        jsx: ts.JsxEmit.React,
        target: ts.ScriptTarget.ES2022,
      },
    }).outputText;
    const exports = {};
    vm.runInNewContext(source, {
      exports,
      require: (id) => {
        if (id in deps) return deps[id];
        if (id.startsWith("./")) {
          const base = new URL(`${id}.tsx`, import.meta.url);
          return load(
            fs.existsSync(base) ? base : new URL(`${id}.ts`, import.meta.url),
          );
        }
        throw Error(id);
      },
      setTimeout,
      clearTimeout,
      TextEncoder,
      crypto: globalThis.crypto,
      console,
      URL,
    });
    return exports;
  }
  deps["@/features/agents/ui/agentActivityChrome"] = load(
    new URL("../agents/ui/agentActivityChrome.ts", import.meta.url),
  );
  deps["@/features/agents/lib/cancelTurnOutcome"] = load(
    new URL("../agents/lib/cancelTurnOutcome.ts", import.meta.url),
  );
  deps["@/features/agents/lib/steerTurnOutcome"] = load(
    new URL("../agents/lib/steerTurnOutcome.ts", import.meta.url),
  );
  deps["@/features/agents/activeConversationAgentTurnSummaries"] = {
    useActiveTurnSummariesForConversation: () => [
      { agentPubkey: selection.agentPubkey, runs: state.turns },
    ],
  };
  state.token = {
    scope: { owner: state.viewer, community: "https://crew.example" },
    workspace_generation: 1,
    identity_generation: 1,
  };
  deps["@/shared/api/ownerOperations"] = {
    captureOwnerOperationScope: async () => state.token,
  };
  deps["@/shared/api/tauri"] = {
    invokeTauri: async (command, input) => {
      state.sends.push({ command, input, requestId: input.payload.requestId });
      return { status: "accepted" };
    },
  };
  deps["@/features/messages/lib/threadForgeViewContextStore"] = {
    useThreadForgeViewContext: () => ({
      channelId: selection.channelId,
      rootEventId: state.root,
      messages: [{ id: state.root, body: "Original task" }],
      profiles: {},
    }),
  };
  deps["@/features/messages/ui/useDeclaredPlansForThread"] = {
    useDeclaredPlansForThread: () => ({
      plans: [
        {
          agentPubkey: selection.agentPubkey,
          agentName: "Worker",
          liveness: "active",
        },
      ],
      conversationId: selection.conversationId,
    }),
  };
  deps["@/features/messages/ui/DeclaredPlansRail"] = {
    DeclaredPlansRail: () => null,
  };
  deps["./ThreadAgentTranscript"] = {
    ThreadAgentTranscript: () =>
      React.createElement("p", null, "Transcript retained"),
  };
  const { ThreadInformationTab: Component } = load(
    new URL("./ThreadInformationTab.tsx", import.meta.url),
  );
  const publishStop = async (target, requestId) => {
    state.sends.push({ target, requestId });
    return { status: "accepted" };
  };
  const result = (overrides = {}) =>
    results.get(selection.agentPubkey)?.({
      type: "cancel_turn",
      status: "sent",
      channelId: selection.channelId,
      conversationId: selection.conversationId,
      turnId: selection.turnId,
      requestId: state.sends.at(-1)?.requestId,
      ...overrides,
    });
  return { Component, state, publishStop, result, listeners, results };
}

const props = {
  channelId: selection.channelId,
  threadRootId: selection.rootEventId,
  tab: "activity",
};
async function choose(view, fireEvent, waitFor) {
  const picker = view.getByRole("combobox", { name: "Activity live run" });
  await waitFor(() => assert.equal(picker.disabled, false));
  fireEvent.change(picker, {
    target: { value: JSON.stringify([selection.sessionId, selection.turnId]) },
  });
}
test("actual Activity requires explicit run choice and sends exact native scoped Stop", async () => {
  const { render, act, fireEvent, waitFor } = await import(
    "@testing-library/react"
  );
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
  await choose(view, fireEvent, waitFor);
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  assert.equal(h.state.sends.length, 1);
  const { command, input } = h.state.sends[0];
  assert.equal(command, "send_scoped_observer_control");
  assert.equal(
    JSON.stringify(input.expectedScope),
    JSON.stringify(h.state.token),
  );
  assert.equal(input.payload.turnId, selection.turnId);
  assert.equal(input.payload.channelId, selection.channelId);
  assert.equal(input.payload.conversationId, selection.conversationId);
  assert.equal(input.agentPubkey, selection.agentPubkey);
  await act(async () => h.result());
  assert.match(view.getByRole("status").textContent, /signal accepted/);
  assert.ok(view.getByText("Transcript retained"));
});

test("actual Activity exposes Steer for the explicitly selected live run", async () => {
  const { render, act, fireEvent, waitFor } = await import(
    "@testing-library/react"
  );
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  await choose(view, fireEvent, waitFor);
  assert.ok(
    view.getByRole("textbox", { name: "Steer selected run" }),
    "the selected live run must expose a steer input",
  );
  assert.ok(
    view.getByRole("button", { name: "Steer selected run" }),
    "the selected live run must expose a steer action",
  );
  await act(async () => {
    fireEvent.change(
      view.getByRole("textbox", { name: "Steer selected run" }),
      {
        target: { value: "Use the selected run" },
      },
    );
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  assert.equal(h.state.sends.length, 1);
  assert.equal(h.state.sends[0].input.payload.type, "steer_turn");
  assert.equal(h.state.sends[0].input.payload.sessionId, selection.sessionId);
  assert.equal(h.state.sends[0].input.payload.turnId, selection.turnId);
  assert.equal(h.state.sends[0].input.payload.prompt, "Use the selected run");
  await act(async () =>
    h.result({
      type: "steer_turn",
      status: "appended",
      sessionId: selection.sessionId,
    }),
  );
  assert.match(view.getByRole("status").textContent, /appended/);
});
test("replacing live run preserves old selection and cannot target successor", async () => {
  const { render, fireEvent, waitFor } = await import("@testing-library/react");
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  await choose(view, fireEvent, waitFor);
  h.state.turns = [{ ...selection, turnId: "successor" }];
  view.rerender(React.createElement(h.Component, props));
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
  assert.equal(
    view.getByRole("combobox", { name: "Activity live run" }).value,
    JSON.stringify([selection.sessionId, selection.turnId]),
  );
  assert.equal(h.state.sends.length, 0);
});
test("changing the thread root retires the selected run before any control", async () => {
  const { act, render, fireEvent, waitFor } = await import(
    "@testing-library/react"
  );
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  await choose(view, fireEvent, waitFor);
  const oldStop = view.getByRole("button", { name: "Stop selected run" });

  h.state.root = "successor-root";
  await act(async () =>
    view.rerender(
      React.createElement(h.Component, {
        ...props,
        threadRootId: "successor-root",
      }),
    ),
  );
  assert.equal(
    view.queryByRole("button", { name: "Stop selected run" }),
    null,
    "a root change must retire the old selected run",
  );
  fireEvent.click(oldStop);
  assert.equal(
    h.state.sends.length,
    0,
    "a detached control from the old root must not publish",
  );
});
test("native owner mismatch prevents selecting an actionable run", async () => {
  const { render, waitFor } = await import("@testing-library/react");
  const h = harness();
  h.state.token.scope.owner = "other";
  const view = render(React.createElement(h.Component, props));
  await waitFor(() =>
    assert.match(
      view.getByRole("status").textContent,
      /Owner or community changed/,
    ),
  );
  assert.equal(
    view.getByRole("combobox", { name: "Activity live run" }).disabled,
    true,
  );
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
});

test("a draft composed for one run is never carried to the run selected next", async () => {
  const { render, act, fireEvent, waitFor } = await import(
    "@testing-library/react"
  );
  const h = harness();
  const second = { ...selection, sessionId: "session-2", turnId: "turn-2" };
  h.state.turns.push(second);
  const view = render(React.createElement(h.Component, props));
  await choose(view, fireEvent, waitFor);
  const draft = view.getByRole("textbox", { name: "Steer selected run" });
  await act(async () =>
    fireEvent.change(draft, {
      target: { value: "guidance for the first run" },
    }),
  );
  const picker = view.getByRole("combobox", { name: "Activity live run" });
  await act(async () =>
    fireEvent.change(picker, {
      target: { value: JSON.stringify([second.sessionId, second.turnId]) },
    }),
  );
  const swapped = view.getByRole("textbox", { name: "Steer selected run" });
  assert.equal(swapped.value, "", "the successor run must start with no draft");
  await act(async () => {
    fireEvent.change(swapped, { target: { value: "guidance for the second" } });
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  assert.equal(h.state.sends.length, 1);
  assert.equal(h.state.sends[0].input.payload.turnId, second.turnId);
  assert.equal(h.state.sends[0].input.payload.sessionId, second.sessionId);
  assert.equal(
    h.state.sends[0].input.payload.prompt,
    "guidance for the second",
  );
});
