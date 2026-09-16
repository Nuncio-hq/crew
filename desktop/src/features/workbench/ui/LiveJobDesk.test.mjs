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

/**
 * The desk is exercised through the real run-control seam: the publisher hook,
 * the compact controls, and the chrome constants are the production modules,
 * so a stub cannot hide a wrong scope, a wrong payload, or wrong copy.
 */
function harness({
  runs,
  owned = true,
  show = true,
  outcome,
  reduceMotion,
} = {}) {
  const results = new Map();
  const state = {
    viewer: selection.viewerPubkey,
    relay: selection.relayUrl,
    owned: owned ? new Set([selection.agentPubkey]) : new Set(),
    turns: runs ?? [{ ...selection }],
    sends: [],
    openedTabs: [],
    stops: [],
    show,
    outcome,
    reduceMotion,
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
    "@/shared/lib/cn": { cn: (...parts) => parts.filter(Boolean).join(" ") },
    "@/shared/ui/button": {
      Button: ({ size, variant, ...props }) =>
        React.createElement("button", { type: "button", ...props }),
    },
    "@/features/messages/lib/messageThreadPanelLayout": {
      THREAD_PANEL_MESSAGE_GUTTER_CLASS: "px-5",
    },
    "@/features/channels/ui/useComposerAgentStop": {
      useComposerAgentStop: (args) => ({
        stopAgent: async (pubkey, name) =>
          state.stops.push({ ...args, pubkey, name }),
      }),
    },
    "motion/react": { useReducedMotion: () => state.reduceMotion ?? false },
    "@/features/agents/recentConversationOutcomes": {
      useRecentOutcomeForConversation: () => state.outcome ?? null,
    },
    "@/features/tool-pane/toolPaneStore": {
      openThreadToolPane: (tab) => state.openedTabs.push(tab),
    },
    "@/features/agents/activeAgentTurnsStore": {
      walkActiveAgentTurns: (fn) =>
        state.turns.forEach((turn) => {
          fn(turn.agentPubkey, turn, 0);
        }),
      subscribeActiveAgentTurns: () => () => {},
    },
    "@/features/agents/controlResultDispatch": {
      subscribeControlResults: (agent, fn) => {
        results.set(agent, fn);
        return () => results.delete(agent);
      },
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
    "../hooks/useLiveJobDesk": {
      useLiveJobDesk: () => ({
        conversationId: selection.conversationId,
        nameFor: () => "Worker",
        runs: state.turns.map((turn) => ({
          agentPubkey: turn.agentPubkey,
          sessionId: turn.sessionId,
          turnId: turn.turnId,
          liveness: "Working",
        })),
        show: state.show,
        targetName: "Worker",
        targetPubkey: selection.agentPubkey,
      }),
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
  const real = (id, relative) => {
    deps[id] = load(new URL(relative, import.meta.url));
  };
  real(
    "@/features/agents/ui/agentActivityChrome",
    "../../agents/ui/agentActivityChrome.ts",
  );
  real(
    "@/features/agents/lib/cancelTurnOutcome",
    "../../agents/lib/cancelTurnOutcome.ts",
  );
  real(
    "@/features/agents/lib/steerTurnOutcome",
    "../../agents/lib/steerTurnOutcome.ts",
  );
  real(
    "@/shared/hooks/escapeSurfaces",
    "../../../shared/hooks/escapeSurfaces.ts",
  );
  real(
    "@/features/tool-pane/ThreadSelectedRunControls",
    "../../tool-pane/ThreadSelectedRunControls.tsx",
  );
  real(
    "@/features/tool-pane/useThreadRunControlPublisher",
    "../../tool-pane/useThreadRunControlPublisher.ts",
  );
  const module = load(new URL("./LiveJobDesk.tsx", import.meta.url));
  return { Component: module.LiveJobDesk, state };
}

const props = { channelId: selection.channelId, threadRootId: "root" };

test("one live run binds Stop to that exact run through the shared seam", async () => {
  const { render, act, waitFor } = await import("@testing-library/react");
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  await waitFor(() =>
    assert.ok(view.queryByRole("button", { name: "Stop run" })),
  );
  assert.match(
    view.getByTestId("live-job-desk-run-agent").textContent,
    /Worker is working/,
  );
  assert.equal(view.queryByRole("button", { name: "Stop Worker" }), null);
  await act(async () => view.getByRole("button", { name: "Stop run" }).click());
  assert.equal(h.state.sends.length, 1);
  const [{ command, input }] = h.state.sends;
  assert.equal(command, "send_scoped_observer_control");
  assert.equal(input.agentPubkey, selection.agentPubkey);
  assert.equal(input.payload.type, "cancel_turn");
  assert.equal(input.payload.conversationId, selection.conversationId);
  assert.equal(input.payload.turnId, selection.turnId);
});

test("one live run steers that exact run from its own labelled input", async () => {
  const { render, act, fireEvent, waitFor } = await import(
    "@testing-library/react"
  );
  const h = harness();
  const view = render(React.createElement(h.Component, props));
  await waitFor(() =>
    assert.ok(view.queryByRole("button", { name: "Steer run" })),
  );
  await act(async () =>
    view.getByRole("button", { name: "Steer run" }).click(),
  );
  // A name of its own: Activity's input keeps "Steer selected run".
  assert.equal(
    view.queryByRole("textbox", { name: "Steer selected run" }),
    null,
  );
  const textbox = view.getByRole("textbox", { name: "Steer this run" });
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

  // Keyboard is a first-class path out of the disclosure, and focus must
  // return to the control that opened it.
  const toggle = view.getByRole("button", { name: "Steer run" });
  await act(async () => dispatchEscape(textbox));
  assert.equal(view.queryByRole("textbox", { name: "Steer this run" }), null);
  assert.equal(dom.window.document.activeElement, toggle);
});

/**
 * The real webview path: a bubbling keydown on the textarea itself, under the
 * focus thread drawer's capture-phase Escape claim. The drawer decides before
 * the key ever reaches the element, so `defaultPrevented` cannot tell it the
 * steer input owns the key — only the escape-owner marker can.
 */
function dispatchEscape(node) {
  node.dispatchEvent(
    new dom.window.KeyboardEvent("keydown", {
      key: "Escape",
      bubbles: true,
      cancelable: true,
    }),
  );
}

function loadModule(relative) {
  const source = ts.transpileModule(
    fs.readFileSync(new URL(relative, import.meta.url), "utf8"),
    {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
      },
    },
  ).outputText;
  const exports = {};
  vm.runInNewContext(source, { exports, require: () => ({}) });
  return exports;
}

function mountDrawerEscapeClaim(closes) {
  const { escapeIsClaimedByNestedOwner: claim } = loadModule(
    "../../../shared/hooks/escapeSurfaces.ts",
  );
  const handler = (event) => {
    if (event.key !== "Escape") return;
    if (claim(event.target)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    closes.push("thread");
  };
  dom.window.addEventListener("keydown", handler, { capture: true });
  return () => dom.window.removeEventListener("keydown", handler, true);
}

test("Escape in the steer input closes it instead of the thread drawer", async () => {
  const { render, act, fireEvent, waitFor } = await import(
    "@testing-library/react"
  );
  const h = harness();
  const closes = [];
  const release = mountDrawerEscapeClaim(closes);
  try {
    const view = render(React.createElement(h.Component, props));
    await waitFor(() =>
      assert.ok(view.queryByRole("button", { name: "Steer run" })),
    );
    await act(async () =>
      view.getByRole("button", { name: "Steer run" }).click(),
    );
    const textbox = view.getByRole("textbox", { name: "Steer this run" });
    fireEvent.change(textbox, { target: { value: "keep going" } });
    await act(async () => dispatchEscape(textbox));
    // The input closed and the thread stayed open.
    assert.equal(view.queryByRole("textbox", { name: "Steer this run" }), null);
    assert.deepEqual(closes, []);
    // A second Escape, now outside the input, still dismisses the thread.
    await act(async () =>
      dispatchEscape(view.getByRole("button", { name: "Steer run" })),
    );
    assert.deepEqual(closes, ["thread"]);
  } finally {
    release();
  }
});

test("a stopped run leaves a bounded neutral trace where the strip was", async () => {
  const { render, act } = await import("@testing-library/react");
  const endedAt = Date.now();
  const h = harness({
    show: false,
    reduceMotion: true,
    outcome: {
      outcome: "cancelled",
      agentPubkey: selection.agentPubkey,
      endedAt,
      channelId: selection.channelId,
    },
  });
  const view = render(React.createElement(h.Component, props));
  assert.match(
    view.getByTestId("live-job-desk-stopped").textContent,
    /Worker · Run stopped/,
  );
  // Bounded: the window is measured from the recorded end time, so a run that
  // ended long ago leaves nothing behind.
  view.unmount();
  const stale = harness({
    show: false,
    reduceMotion: true,
    outcome: {
      outcome: "cancelled",
      agentPubkey: selection.agentPubkey,
      endedAt: endedAt - 60_000,
      channelId: selection.channelId,
    },
  });
  const staleView = render(React.createElement(stale.Component, props));
  assert.equal(staleView.queryByTestId("live-job-desk-stopped"), null);
  await act(async () => {});
});

test("a completed run leaves no stopped trace", async () => {
  const { render } = await import("@testing-library/react");
  const h = harness({
    show: false,
    outcome: {
      outcome: "completed",
      agentPubkey: selection.agentPubkey,
      endedAt: Date.now(),
      channelId: selection.channelId,
    },
  });
  const view = render(React.createElement(h.Component, props));
  assert.equal(view.queryByTestId("live-job-desk-stopped"), null);
});

test("a live run this viewer does not own keeps the agent-scoped controls", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness({ owned: false });
  const view = render(React.createElement(h.Component, props));
  assert.equal(view.queryByRole("button", { name: "Stop run" }), null);
  assert.match(
    view.getByTestId("live-job-desk").textContent,
    /Worker is working — steer if stuck/,
  );
  await act(async () =>
    view.getByRole("button", { name: "Stop Worker" }).click(),
  );
  assert.equal(h.state.sends.length, 0);
  assert.equal(h.state.stops.length, 1);
  assert.equal(h.state.stops[0].pubkey, selection.agentPubkey);
});

test("no known run identity keeps the honest agent-scoped strip", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness({ runs: [] });
  const view = render(React.createElement(h.Component, props));
  assert.ok(view.getByTestId("live-job-desk-steer"));
  assert.equal(view.queryByRole("button", { name: "Stop run" }), null);
  await act(async () => view.getByTestId("live-job-desk-stop").click());
  assert.equal(h.state.stops.length, 1);
  assert.equal(h.state.sends.length, 0);
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
