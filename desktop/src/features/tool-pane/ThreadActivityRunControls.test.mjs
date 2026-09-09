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
        if (id.startsWith("./"))
          return load(new URL(`${id}.tsx`, import.meta.url));
        throw Error(id);
      },
      setTimeout,
      clearTimeout,
      crypto: globalThis.crypto,
      console,
      URL,
    });
    return exports;
  }
  deps["@/features/agents/lib/cancelTurnOutcome"] = load(
    new URL("../agents/lib/cancelTurnOutcome.ts", import.meta.url),
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
      rootEventId: selection.rootEventId,
      messages: [{ id: selection.rootEventId, body: "Original task" }],
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
