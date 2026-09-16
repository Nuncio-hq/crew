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

function harness({ runs } = {}) {
  const listeners = new Set(),
    results = new Map();
  const state = {
    viewer: selection.viewerPubkey,
    relay: selection.relayUrl,
    owned: new Set([selection.agentPubkey]),
    turns: runs ?? [{ ...selection }],
    sends: [],
    openedTabs: [],
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
    "@/shared/lib/pubkey": {
      normalizePubkey: (value) => value.toLowerCase(),
      truncatePubkey: (value) => value.slice(0, 8),
    },
    "@/features/agents/conversationId": {
      deriveAgentConversationIdOrNull: () => selection.conversationId,
    },
    "@/features/agents/activeAgentTurnsStore": {
      walkActiveAgentTurns: (fn) =>
        state.turns.forEach((turn) => {
          fn(turn.agentPubkey, turn, 0);
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
    "@/features/agents/activeConversationAgentTurnSummaries": {
      useActiveTurnSummariesForConversation: () =>
        state.turns.length === 0
          ? []
          : [
              {
                agentPubkey: selection.agentPubkey,
                progressLabel: "Working",
                runs: state.turns,
              },
            ],
    },
    "./toolPaneStore": {
      openThreadToolPane: (tab) => state.openedTabs.push(tab),
    },
    "@/shared/api/ownerOperations": {
      captureOwnerOperationScope: async () => ({
        scope: { owner: state.viewer, community: "https://crew.example" },
        workspace_generation: 1,
        identity_generation: 1,
      }),
    },
    "@/shared/api/tauri": {
      invokeTauri: async (command, input) => {
        state.sends.push({ command, input });
        return { status: "accepted" };
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
      // Mirrors the build's automatic JSX runtime for files that do not
      // import React themselves.
      React,
      setTimeout,
      clearTimeout,
      TextEncoder,
      crypto: globalThis.crypto,
      console,
      URL,
    });
    return exports;
  }
  // The real chrome module, so a stub cannot hide wrong copy.
  deps["@/features/agents/ui/agentActivityChrome"] = load(
    new URL("../agents/ui/agentActivityChrome.ts", import.meta.url),
  );
  deps["@/features/agents/lib/cancelTurnOutcome"] = load(
    new URL("../agents/lib/cancelTurnOutcome.ts", import.meta.url),
  );
  deps["@/features/agents/lib/steerTurnOutcome"] = load(
    new URL("../agents/lib/steerTurnOutcome.ts", import.meta.url),
  );
  const module = load(
    new URL("./ThreadInlineRunControls.tsx", import.meta.url),
  );
  return { module, Component: module.ThreadInlineRunControls, state, results };
}

const props = {
  channelId: selection.channelId,
  rootEventId: selection.rootEventId,
  agentNames: { [selection.agentPubkey]: "Worker" },
};

test("the inline strip is absent for a thread with no live run", async () => {
  const { render } = await import("@testing-library/react");
  const h = harness({ runs: [] });
  const view = render(React.createElement(h.Component, props));
  assert.equal(view.queryByTestId("thread-inline-run-controls"), null);
});

test("one live run offers Stop bound to that exact run through the shared seam", async () => {
  const { render, act, waitFor } = await import("@testing-library/react");
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  await waitFor(() =>
    assert.ok(view.queryByRole("button", { name: "Stop run" })),
  );
  assert.match(
    view.getByTestId("thread-inline-run-agent").textContent,
    /Worker is working/,
  );
  assert.match(view.container.textContent, /Working/);
  await act(async () => view.getByRole("button", { name: "Stop run" }).click());
  assert.equal(h.state.sends.length, 1);
  const [{ command, input }] = h.state.sends;
  assert.equal(command, "send_scoped_observer_control");
  assert.equal(input.agentPubkey, selection.agentPubkey);
  assert.equal(input.payload.type, "cancel_turn");
  assert.equal(input.payload.channelId, selection.channelId);
  assert.equal(input.payload.conversationId, selection.conversationId);
  assert.equal(input.payload.turnId, selection.turnId);
});

test("one live run steers that exact run, and Escape closes the input", async () => {
  const { render, act, fireEvent, waitFor } = await import(
    "@testing-library/react"
  );
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  await waitFor(() =>
    assert.ok(view.queryByRole("button", { name: "Steer run" })),
  );
  const toggle = view.getByRole("button", { name: "Steer run" });
  await act(async () => toggle.click());
  const textbox = view.getByRole("textbox", { name: "Steer selected run" });
  await act(async () => {
    fireEvent.change(textbox, {
      target: { value: "stay on the failing test" },
    });
    view.getByRole("button", { name: "Send steer" }).click();
  });
  assert.equal(h.state.sends.length, 1);
  const [{ input }] = h.state.sends;
  assert.equal(input.payload.type, "steer_turn");
  assert.equal(input.payload.sessionId, selection.sessionId);
  assert.equal(input.payload.turnId, selection.turnId);
  assert.equal(input.payload.prompt, "stay on the failing test");

  await act(async () => fireEvent.keyDown(textbox, { key: "Escape" }));
  assert.equal(
    view.queryByRole("textbox", { name: "Steer selected run" }),
    null,
  );
  assert.equal(dom.window.document.activeElement, toggle);
});

test("several live runs point at Activity instead of guessing a target", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness({
    runs: [
      { ...selection },
      { ...selection, sessionId: "session-2", turnId: "turn-2" },
    ],
  });
  const view = render(React.createElement(h.Component, props));
  assert.equal(view.queryByRole("button", { name: "Stop run" }), null);
  assert.match(view.container.textContent, /2 runs/);
  assert.doesNotMatch(view.container.textContent, /agents? working/);
  await act(async () =>
    view.getByRole("button", { name: "Open Activity" }).click(),
  );
  assert.deepEqual(h.state.openedTabs, ["activity"]);
  assert.equal(h.state.sends.length, 0);
});

test("liveRunsForThread drops incomplete and duplicate run identities", () => {
  const { liveRunsForThread } = harness().module;
  // Cross-realm values: compare shape, not identity.
  assert.equal(
    JSON.stringify(
      liveRunsForThread([
        {
          agentPubkey: "agent",
          runs: [
            { sessionId: "s", turnId: "t" },
            { sessionId: "s", turnId: "t" },
            { sessionId: "s", turnId: null },
            { sessionId: "", turnId: "t" },
          ],
        },
        { agentPubkey: "other" },
      ]).map((run) => [run.agentPubkey, run.sessionId, run.turnId]),
    ),
    JSON.stringify([["agent", "s", "t"]]),
  );
});
