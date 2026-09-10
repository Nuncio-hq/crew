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
        throw Error(id);
      },
      setTimeout,
      clearTimeout,
      crypto: globalThis.crypto,
      console,
    });
    return exports;
  }
  deps["@/features/agents/lib/cancelTurnOutcome"] = load(
    new URL("../agents/lib/cancelTurnOutcome.ts", import.meta.url),
  );
  const { ThreadSelectedRunControls: Component } = load(
    new URL("./ThreadSelectedRunControls.tsx", import.meta.url),
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
test("same-tick Stop publishes once with immutable exact turn and waits for correlated native result", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  const button = view.getByRole("button", { name: "Stop selected run" });
  await act(async () => {
    button.click();
    button.click();
  });
  assert.equal(h.state.sends.length, 1);
  assert.equal(h.state.sends[0].target.turnId, "turn");
  assert.ok(h.state.sends[0].requestId);
  assert.match(view.getByRole("status").textContent, /Waiting/);
  await act(async () => h.result({ turnId: "replacement" }));
  assert.match(view.getByRole("status").textContent, /Waiting/);
  await act(async () => h.result({ requestId: "old" }));
  assert.match(view.getByRole("status").textContent, /Waiting/);
  await act(async () => h.result());
  assert.match(view.getByRole("status").textContent, /signal accepted/);
});
test("historical, missing and wrong-channel runs have no Stop action", async () => {
  const { render } = await import("@testing-library/react");
  const h = harness();
  h.state.turns = [];
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
  h.state.turns = [{ ...selection, channelId: "elsewhere" }];
  view.rerender(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
  view.rerender(
    React.createElement(h.Component, {
      selection: { ...selection, turnId: "" },
      publishStop: h.publishStop,
    }),
  );
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
});
test("click rechecks live turn even before store subscription renders", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  const button = view.getByRole("button", { name: "Stop selected run" });
  h.state.turns = [];
  await act(async () => button.click());
  assert.equal(h.state.sends.length, 0);
});
test("selection replacement fences pending completion and releases result listener", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  const old = h.results.get(selection.agentPubkey);
  const next = { ...selection, turnId: "next" };
  h.state.turns = [next];
  view.rerender(
    React.createElement(h.Component, {
      selection: next,
      publishStop: h.publishStop,
    }),
  );
  await act(async () =>
    old({
      type: "cancel_turn",
      status: "sent",
      channelId: "channel",
      conversationId: "thread",
      turnId: "turn",
      requestId: h.state.sends[0].requestId,
    }),
  );
  assert.equal(view.queryByRole("status"), null);
  assert.equal(h.results.size, 0);
});
test("owner and relay changes remove live action", async () => {
  const { render } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  assert.ok(view.getByRole("button", { name: "Stop selected run" }));
  h.state.viewer = "other";
  view.rerender(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
  h.state.viewer = selection.viewerPubkey;
  h.state.relay = "wss://other";
  view.rerender(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
});

test("unknown transport failure cannot silently enable replay; confirmed no-send may retry", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  let calls = 0;
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: async () => {
        calls++;
        throw Error("network broke");
      },
    }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  assert.match(view.getByRole("status").textContent, /unconfirmed/);
  assert.equal(
    view.getByRole("button", { name: "Stop selected run" }).disabled,
    true,
  );
  assert.equal(calls, 1);
  view.unmount();
  const second = render(
    React.createElement(h.Component, {
      selection,
      publishStop: async () => ({
        status: "not_attempted",
        message: "Scope changed before sending.",
      }),
    }),
  );
  await act(async () =>
    second.getByRole("button", { name: "Stop selected run" }).click(),
  );
  assert.equal(
    second.getByRole("button", { name: "Stop selected run" }).disabled,
    false,
  );
});
test("ownership revocation retires an already pending result", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  h.state.owned = new Set();
  view.rerender(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  assert.equal(h.results.size, 0);
  assert.equal(view.queryByRole("button", { name: "Stop selected run" }), null);
  assert.equal(view.queryByRole("status"), null);
});

test("ownership refresh and unrelated membership changes preserve a pending claim", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  const listener = h.results.get(selection.agentPubkey);
  for (const owned of [
    new Set([selection.agentPubkey]),
    new Set([selection.agentPubkey, "c".repeat(64)]),
  ]) {
    h.state.owned = owned;
    view.rerender(
      React.createElement(h.Component, {
        selection,
        publishStop: h.publishStop,
      }),
    );
    assert.match(view.getByRole("status").textContent, /Waiting/);
    await act(async () =>
      view.getByRole("button", { name: "Stop selected run" }).click(),
    );
    assert.equal(h.state.sends.length, 1);
    assert.equal(h.results.get(selection.agentPubkey), listener);
  }
  await act(async () => h.result());
  assert.match(view.getByRole("status").textContent, /signal accepted/);
});

test("revoke and regrant cannot settle the new gate with an old result", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  const old = h.results.get(selection.agentPubkey);
  const oldRequest = h.state.sends[0].requestId;
  h.state.owned = new Set();
  view.rerender(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  h.state.owned = new Set([selection.agentPubkey]);
  view.rerender(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  await act(async () =>
    old({
      type: "cancel_turn",
      status: "sent",
      channelId: selection.channelId,
      conversationId: selection.conversationId,
      turnId: selection.turnId,
      requestId: oldRequest,
    }),
  );
  assert.match(view.getByRole("status").textContent, /Waiting/);
  await act(async () => h.result());
  assert.match(view.getByRole("status").textContent, /signal accepted/);
});

test("queued cancellation feedback reports the actual harness outcome", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, { selection, publishStop: h.publishStop }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop selected run" }).click(),
  );
  await act(async () => h.result({ status: "cancelled_queued" }));
  assert.match(
    view.getByRole("status").textContent,
    /Queued work was cancelled/,
  );
});
