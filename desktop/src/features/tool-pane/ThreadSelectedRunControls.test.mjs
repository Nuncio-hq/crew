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
    results = new Map(),
    timers = [];
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
      // Controlled timers: the UI budget must be crossable without waiting
      // out the real one, and a late terminal outcome must be observable.
      setTimeout: (fn, ms) => {
        const timer = { fn, ms, cancelled: false, fired: false };
        timers.push(timer);
        return timer;
      },
      clearTimeout: (timer) => {
        if (timer) timer.cancelled = true;
      },
      TextEncoder,
      crypto: globalThis.crypto,
      console,
    });
    return exports;
  }
  deps["@/shared/hooks/escapeSurfaces"] = load(
    new URL("../../shared/hooks/escapeSurfaces.ts", import.meta.url),
  );
  deps["@/features/agents/ui/agentActivityChrome"] = load(
    new URL("../agents/ui/agentActivityChrome.ts", import.meta.url),
  );
  deps["@/features/agents/lib/cancelTurnOutcome"] = load(
    new URL("../agents/lib/cancelTurnOutcome.ts", import.meta.url),
  );
  deps["@/features/agents/lib/steerTurnOutcome"] = load(
    new URL("../agents/lib/steerTurnOutcome.ts", import.meta.url),
  );
  const { ThreadSelectedRunControls: Component } = load(
    new URL("./ThreadSelectedRunControls.tsx", import.meta.url),
  );
  const publishStop = async (target, requestId) => {
    state.sends.push({ target, requestId });
    return { status: "accepted" };
  };
  const publishSteer = async (target, requestId, prompt) => {
    state.sends.push({ target, requestId, prompt });
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
  const steerResult = (overrides = {}) =>
    results.get(selection.agentPubkey)?.({
      type: "steer_turn",
      status: "appended",
      channelId: selection.channelId,
      conversationId: selection.conversationId,
      sessionId: selection.sessionId,
      turnId: selection.turnId,
      requestId: state.sends.at(-1)?.requestId,
      ...overrides,
    });
  const runTimers = (predicate = () => true) => {
    for (const timer of [...timers]) {
      if (timer.cancelled || timer.fired || !predicate(timer)) continue;
      timer.fired = true;
      timer.fn();
    }
  };
  return {
    Component,
    steerBudget: deps["@/features/agents/lib/steerTurnOutcome"],
    state,
    timers,
    runTimers,
    publishStop,
    publishSteer,
    result,
    steerResult,
    listeners,
    results,
  };
}

test("Steer Enter targets the selected session and turn; Shift+Enter does not submit", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  const textbox = view.getByRole("textbox", { name: "Steer selected run" });
  fireEvent.change(textbox, { target: { value: "first" } });
  fireEvent.keyDown(textbox, { key: "Enter", shiftKey: true });
  assert.equal(h.state.sends.length, 0);
  await act(async () => fireEvent.keyDown(textbox, { key: "Enter" }));
  assert.equal(h.state.sends.length, 1);
  assert.equal(h.state.sends[0].target.sessionId, selection.sessionId);
  assert.equal(h.state.sends[0].target.turnId, selection.turnId);
  assert.equal(h.state.sends[0].prompt, "first");
  await act(async () => h.steerResult());
  assert.match(view.getByRole("status").textContent, /appended/);
});
test("unconfirmed Steer result keeps replay disabled", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  const textbox = view.getByRole("textbox", { name: "Steer selected run" });
  await act(async () => {
    fireEvent.change(textbox, { target: { value: "may have applied" } });
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  await act(async () => h.steerResult({ status: "unconfirmed" }));
  assert.match(view.getByRole("status").textContent, /unconfirmed/);
  assert.equal(
    view.getByRole("button", { name: "Steer selected run" }).disabled,
    true,
  );
});
test("rejected Steer feedback preserves the native rejection reason", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  await act(async () => {
    fireEvent.change(
      view.getByRole("textbox", { name: "Steer selected run" }),
      { target: { value: "explain the failure" } },
    );
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  await act(async () =>
    h.steerResult({
      status: "rejected",
      error: "selected adapter does not support strict steering",
    }),
  );
  assert.match(
    view.getByRole("status").textContent,
    /selected adapter does not support strict steering/,
  );
  assert.equal(
    view.getByRole("button", { name: "Steer selected run" }).disabled,
    false,
  );
});
test("rejected Steer feedback falls back when the native reason is absent", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  await act(async () => {
    fireEvent.change(
      view.getByRole("textbox", { name: "Steer selected run" }),
      { target: { value: "retry safely" } },
    );
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  await act(async () => h.steerResult({ status: "rejected", error: " " }));
  assert.match(
    view.getByRole("status").textContent,
    /The selected runtime rejected Steer\. You can retry\./,
  );
});
test("rejected Steer feedback ignores a malformed native reason", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  await act(async () => {
    fireEvent.change(
      view.getByRole("textbox", { name: "Steer selected run" }),
      { target: { value: "retry safely" } },
    );
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  await act(async () => h.steerResult({ status: "rejected", error: 503 }));
  assert.match(
    view.getByRole("status").textContent,
    /The selected runtime rejected Steer\. You can retry\./,
  );
});
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

test("the Steer UI budget outlasts the adapter request deadline", async () => {
  const { STEER_UI_BUDGET_MS, ADAPTER_STRICT_STEER_DEADLINE_MS } =
    harness().steerBudget;
  assert.ok(
    STEER_UI_BUDGET_MS > ADAPTER_STRICT_STEER_DEADLINE_MS,
    "an equal budget makes the adapter's expired answer unobservable",
  );
});
test("a terminal Steer outcome after the UI budget releases the claim", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  await act(async () => {
    fireEvent.change(
      view.getByRole("textbox", { name: "Steer selected run" }),
      { target: { value: "hold the line" } },
    );
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  await act(async () => h.runTimers());
  assert.match(view.getByRole("status").textContent, /unconfirmed/);
  assert.equal(
    view.getByRole("button", { name: "Steer selected run" }).disabled,
    true,
  );
  await act(async () => h.steerResult({ status: "expired" }));
  assert.match(view.getByRole("status").textContent, /expired/);
  assert.equal(
    view.getByRole("button", { name: "Steer selected run" }).disabled,
    false,
  );
  // The correlation is terminal now: a repeat frame must not reopen it.
  await act(async () => h.steerResult({ status: "appended" }));
  assert.match(view.getByRole("status").textContent, /expired/);
});
test("unmounting disposes a Steer correlation that outlived its budget", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  await act(async () => {
    fireEvent.change(
      view.getByRole("textbox", { name: "Steer selected run" }),
      { target: { value: "hold the line" } },
    );
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  await act(async () => h.runTimers());
  assert.equal(h.results.size, 1);
  await act(async () => view.unmount());
  assert.equal(h.results.size, 0);
});
test("an unconfirmed Steer leaves Stop available as the way back", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  await act(async () => {
    fireEvent.change(
      view.getByRole("textbox", { name: "Steer selected run" }),
      { target: { value: "may have applied" } },
    );
    view.getByRole("button", { name: "Steer selected run" }).click();
  });
  await act(async () => h.steerResult({ status: "unconfirmed" }));
  const stopButton = view.getByRole("button", { name: "Stop selected run" });
  assert.equal(stopButton.disabled, false);
  await act(async () => stopButton.click());
  assert.equal(h.state.sends.length, 2);
  assert.equal(h.state.sends[1].target.turnId, selection.turnId);
});
test("the Steer composer enforces the native byte cap, not a character cap", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
    }),
  );
  const textbox = view.getByRole("textbox", { name: "Steer selected run" });
  // 8193 two-byte characters: under a 16384 character cap, over the byte cap.
  const overBudget = "é".repeat(8193);
  await act(async () => {
    fireEvent.change(textbox, { target: { value: overBudget } });
  });
  assert.equal(textbox.value.length, 8193);
  assert.equal(
    view.getByRole("button", { name: "Steer selected run" }).disabled,
    true,
  );
  assert.match(view.container.textContent, /2 bytes over the 16384 byte limit/);
  await act(async () => fireEvent.keyDown(textbox, { key: "Enter" }));
  assert.equal(h.state.sends.length, 0);
  await act(async () => {
    fireEvent.change(textbox, { target: { value: "é".repeat(8192) } });
  });
  assert.match(view.container.textContent, /0 bytes left/);
  assert.equal(
    view.getByRole("button", { name: "Steer selected run" }).disabled,
    false,
  );
});

/**
 * Activity's own steer input has no disclosure to dismiss, but it must still
 * claim Escape: a capture-phase surface above it would otherwise read the
 * operator's dismissal as "close the whole thread panel".
 */
test("Escape in the Activity steer input closes nothing and keeps the panel", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const closes = [];
  const escapeSurfaces = loadEscapeSurfaces();
  const handler = (event) => {
    if (event.key !== "Escape") return;
    if (escapeSurfaces.escapeIsClaimedByNestedOwner(event.target)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    closes.push("thread");
  };
  dom.window.addEventListener("keydown", handler, { capture: true });
  try {
    const view = render(
      React.createElement(h.Component, {
        selection,
        publishStop: h.publishStop,
        publishSteer: h.publishSteer,
      }),
    );
    const textbox = view.getByRole("textbox", { name: "Steer selected run" });
    await act(async () =>
      textbox.dispatchEvent(
        new dom.window.KeyboardEvent("keydown", {
          key: "Escape",
          bubbles: true,
          cancelable: true,
        }),
      ),
    );
    assert.ok(view.getByRole("textbox", { name: "Steer selected run" }));
    assert.deepEqual(closes, []);
  } finally {
    dom.window.removeEventListener("keydown", handler, true);
  }
});

/**
 * Compact mode (LiveJobDesk) keeps the steer input behind a "Steer run"
 * disclosure. After clicking the toggle, focus stays on the toggle button —
 * not the textarea — so Escape there must still close the disclosure and
 * refocus the toggle instead of falling through to an ancestor (the thread
 * drawer) and closing the whole thread.
 */
test("Escape on the compact toggle closes only the disclosure and refocuses it", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const escapeSurfaces = loadEscapeSurfaces();
  let closes = 0;
  const handler = (event) => {
    if (event.key !== "Escape") return;
    if (escapeSurfaces.escapeIsClaimedByNestedOwner(event.target)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    closes++;
  };
  dom.window.addEventListener("keydown", handler, { capture: true });
  try {
    const view = render(
      React.createElement(h.Component, {
        selection,
        publishStop: h.publishStop,
        publishSteer: h.publishSteer,
        compact: true,
      }),
    );
    const toggle = view.getByRole("button", { name: "Steer run" });
    await act(async () => toggle.click());
    assert.ok(view.getByRole("textbox", { name: "Steer this run" }));
    toggle.focus();
    assert.equal(document.activeElement, toggle);

    await act(async () =>
      toggle.dispatchEvent(
        new dom.window.KeyboardEvent("keydown", {
          key: "Escape",
          bubbles: true,
          cancelable: true,
        }),
      ),
    );
    assert.equal(
      closes,
      0,
      "the disclosure's Escape owner must keep the key from the drawer",
    );
    assert.equal(
      view.queryByRole("textbox", { name: "Steer this run" }),
      null,
      "the disclosure closes",
    );
    assert.equal(document.activeElement, toggle, "focus returns to Steer run");

    await act(async () =>
      toggle.dispatchEvent(
        new dom.window.KeyboardEvent("keydown", {
          key: "Escape",
          bubbles: true,
          cancelable: true,
        }),
      ),
    );
    assert.equal(
      closes,
      1,
      "with the disclosure closed, Escape falls through to the drawer",
    );
  } finally {
    dom.window.removeEventListener("keydown", handler, true);
  }
});

test("a modified Escape still closes the compact disclosure", async () => {
  const { render, act } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
      compact: true,
    }),
  );
  await act(async () =>
    view.getByRole("button", { name: "Steer run" }).click(),
  );
  const textbox = view.getByRole("textbox", { name: "Steer this run" });
  await act(async () =>
    textbox.dispatchEvent(
      new dom.window.KeyboardEvent("keydown", {
        key: "Escape",
        metaKey: true,
        bubbles: true,
        cancelable: true,
      }),
    ),
  );
  assert.equal(
    view.queryByRole("textbox", { name: "Steer this run" }),
    null,
    "a modified Escape closes the disclosure just like a bare one",
  );
});

test("Enter still sends from the compact textarea; Enter on the toggle does nothing", async () => {
  const { render, act, fireEvent } = await import("@testing-library/react");
  const h = harness();
  const view = render(
    React.createElement(h.Component, {
      selection,
      publishStop: h.publishStop,
      publishSteer: h.publishSteer,
      compact: true,
    }),
  );
  const toggle = view.getByRole("button", { name: "Steer run" });
  await act(async () => toggle.click());
  await act(async () => fireEvent.keyDown(toggle, { key: "Enter" }));
  assert.equal(h.state.sends.length, 0, "Enter on the toggle does not send");
  const textbox = view.getByRole("textbox", { name: "Steer this run" });
  fireEvent.change(textbox, { target: { value: "go" } });
  await act(async () => fireEvent.keyDown(textbox, { key: "Enter" }));
  assert.equal(h.state.sends.length, 1);
  assert.equal(h.state.sends[0].prompt, "go");
});

function loadEscapeSurfaces() {
  const source = ts.transpileModule(
    fs.readFileSync(
      new URL("../../shared/hooks/escapeSurfaces.ts", import.meta.url),
      "utf8",
    ),
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
