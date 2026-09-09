import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";
import * as React from "react";
import ts from "typescript";
import * as input from "../lib/userInput.ts";
import * as authority from "../../agents/userInputAttentionProjection.ts";
import * as hydration from "../../agents/durableActionHydration.ts";
import * as ancestry from "../../agents/receiptParentLookup.ts";
import * as threading from "../../messages/lib/threading.ts";
import * as filters from "../../../shared/api/relayChannelFilters.ts";
import * as kinds from "../../../shared/constants/kinds.ts";
import * as relayUrl from "../../../shared/lib/normalizeRelayUrl.ts";
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
const OWNER = "a".repeat(64),
  AGENT = "b".repeat(64);
const CHANNEL_A = "11111111-1111-4111-8111-111111111111",
  CHANNEL_B = "22222222-2222-4222-8222-222222222222";
function deferred() {
  let resolve, reject;
  const promise = new Promise((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}
function fixture(channel, digit) {
  const parent = {
    id: digit.repeat(64),
    kind: 9,
    pubkey: OWNER,
    created_at: 1,
    content: "Trigger",
    sig: "",
    tags: [
      ["h", channel],
      ["p", AGENT],
    ],
  };
  const request = {
    id: (digit === "c" ? "e" : "f").repeat(64),
    kind: kinds.KIND_AGENT_USER_INPUT_REQUESTED,
    pubkey: AGENT,
    created_at: 2,
    sig: "",
    tags: [
      ["h", channel],
      ["p", OWNER],
      ["e", parent.id, "", "reply"],
    ],
    content: JSON.stringify({
      request_id: digit,
      channel_id: channel,
      session_id: `session-${digit}`,
      turn_id: `turn-${digit}`,
      engine: "codex",
      questions: [
        {
          id: "q",
          header: "Choice",
          question: "Choose",
          options: [{ value: "yes", label: "Yes", description: "Continue" }],
        },
      ],
    }),
  };
  return { parent, request };
}
function loadHarness() {
  const a = fixture(CHANNEL_A, "c"),
    b = fixture(CHANNEL_B, "d");
  const state = {
    channel: CHANNEL_A,
    owner: OWNER,
    relay: "wss://crew.example",
    events: [a.parent, a.request, b.parent, b.request],
    sends: [],
    listeners: new Set(),
  };
  state.owned = new Set([AGENT]);
  const deps = {
    react: React,
    "@/shared/api/hooks": {
      useIdentityQuery: () => ({
        data: { pubkey: state.owner },
        isLoading: false,
      }),
    },
    "@/features/communities/useCommunities": {
      useCommunities: () => ({ activeCommunity: { relayUrl: state.relay } }),
    },
    "@/shared/lib/normalizeRelayUrl": relayUrl,
    "@/shared/api/relayClient": {
      relayClient: {
        subscribeLive: async (filter, onEvent) => {
          const listener = { filter, onEvent };
          state.listeners.add(listener);
          return async () => state.listeners.delete(listener);
        },
        fetchEvents: async (filter) =>
          state.events.filter(
            (event) =>
              (filter.ids
                ? filter.ids.includes(event.id)
                : filter.kinds.includes(event.kind) &&
                  event.tags.some(
                    (tag) => tag[0] === "h" && filter["#h"].includes(tag[1]),
                  )) &&
              (filter.until === undefined || event.created_at <= filter.until),
          ),
      },
    },
    "@/features/agents/receiptParentLookup": ancestry,
    "@/shared/api/relayChannelFilters": filters,
    "@/features/agents/durableActionHydration": hydration,
    "@/features/messages/lib/threading": threading,
    "@/shared/api/tauriUserInput": {
      sendChannelUserInputAnswer: (...args) => {
        const task = deferred();
        state.sends.push({ args, ...task });
        return task.promise;
      },
    },
    "@/shared/constants/kinds": kinds,
    "@/features/channels/lib/userInput": input,
    "@/features/agents/userInputAttentionProjection": authority,
    "@/features/home/useOwnedAgentPubkeys": {
      useCurrentOwnedAgentPubkeys: () => state.owned,
    },
    "@/features/agents/needsYouStore": { clearUserInputRequests: () => {} },
  };
  function loadFile(file) {
    const exports = {};
    vm.runInNewContext(
      ts.transpileModule(
        fs.readFileSync(new URL(file, import.meta.url), "utf8"),
        {
          compilerOptions: {
            module: ts.ModuleKind.CommonJS,
            target: ts.ScriptTarget.ES2022,
          },
        },
      ).outputText,
      {
        exports,
        console,
        setTimeout,
        clearTimeout,
        Error,
        require: (key) => {
          if (key.startsWith("./")) return loadFile(`${key}.ts`);
          assert.ok(key in deps, `unmocked dependency ${key}`);
          return deps[key];
        },
      },
    );
    return exports;
  }
  return {
    state,
    useHook: loadFile("./useChannelUserInput.ts").useChannelUserInput,
  };
}
async function mount() {
  const { renderHook, act } = await import("@testing-library/react");
  const harness = loadHarness();
  const view = renderHook(() => harness.useHook(harness.state.channel));
  await act(async () => {});
  assert.equal(
    view.result.current.pending.length,
    1,
    "real authorized hydration is ready",
  );
  return { ...harness, view, act };
}
test("same-tick answer replay publishes once and successful replay stays inert", async () => {
  const { state, view, act } = await mount();
  const request = view.result.current.pending[0],
    answer = view.result.current.answer;
  let first, second;
  await act(async () => {
    first = answer(request, { q: "yes" });
    second = answer(request, { q: "yes" });
  });
  assert.equal(state.sends.length, 1, "same-tick duplicate published twice");
  await act(async () => {
    state.sends[0].resolve({ eventId: "answer" });
    await first;
    await second;
  });
  await act(async () => answer(request, { q: "yes" }));
  assert.equal(
    state.sends.length,
    1,
    "successful stale callback republished answer",
  );
});
test("publication failure preserves error and releases a real retry", async () => {
  const { state, view, act } = await mount();
  const request = view.result.current.pending[0];
  let first;
  await act(async () => {
    first = view.result.current.answer(request, { q: "yes" });
  });
  await act(async () => {
    state.sends[0].reject(new Error("relay unavailable"));
    await first;
  });
  assert.equal(
    view.result.current.errors[request.event.id],
    "relay unavailable",
  );
  assert.equal(view.result.current.pending.length, 1);
  let retry;
  await act(async () => {
    retry = view.result.current.answer(request, { q: "yes" });
  });
  assert.equal(state.sends.length, 2);
  await act(async () => {
    state.sends[1].resolve({ eventId: "answer" });
    await retry;
  });
  assert.equal(view.result.current.pending.length, 0);
});
test("late old-channel completion cannot clear a newer question's sending state", async () => {
  const { state, view, act } = await mount();
  let oldAnswer, newAnswer;
  await act(async () => {
    oldAnswer = view.result.current.answer(view.result.current.pending[0], {
      q: "yes",
    });
  });
  state.channel = CHANNEL_B;
  await act(async () => view.rerender());
  const request = view.result.current.pending[0];
  assert.equal(request.request.channel_id, CHANNEL_B);
  await act(async () => {
    newAnswer = view.result.current.answer(request, { q: "yes" });
  });
  await act(async () => {
    state.sends[0].resolve({ eventId: "old-answer" });
    await oldAnswer;
  });
  assert.equal(
    view.result.current.sendingRequestId,
    request.event.id,
    "old completion cleared newer request",
  );
  assert.equal(view.result.current.sentRequestIds.size, 0);
  await act(async () => {
    state.sends[1].resolve({ eventId: "new-answer" });
    await newAnswer;
  });
});
test("a callback retained across relay or viewer change cannot publish", async () => {
  for (const field of ["relay", "owner"]) {
    const { state, view, act } = await mount();
    const answer = view.result.current.answer,
      request = view.result.current.pending[0];
    state[field] = field === "relay" ? "wss://other.example" : "9".repeat(64);
    await act(async () => view.rerender());
    await act(async () => {
      void answer(request, { q: "yes" });
    });
    assert.equal(state.sends.length, 0, `retired ${field} callback published`);
    view.unmount();
  }
});
test("a resolved request cannot publish from its old callback", async () => {
  const { state, view, act } = await mount();
  const answer = view.result.current.answer,
    request = view.result.current.pending[0];
  await act(async () => {
    for (const listener of state.listeners)
      listener.onEvent({
        id: "8".repeat(64),
        kind: kinds.KIND_AGENT_USER_INPUT_RESOLVED,
        pubkey: AGENT,
        created_at: 3,
        sig: "",
        tags: [
          ["h", CHANNEL_A],
          ["e", request.event.id],
          ["p", OWNER],
        ],
        content: JSON.stringify({
          request_event_id: request.event.id,
          outcome: "cancelled",
        }),
      });
  });
  assert.equal(view.result.current.pending.length, 0);
  await act(async () => {
    void answer(request, { q: "yes" });
  });
  assert.equal(state.sends.length, 0, "resolved question published again");
});

test("ownership revocation immediately retires the prior action before hydration", async () => {
  const { state, view, act } = await mount();
  const answer = view.result.current.answer,
    request = view.result.current.pending[0];
  act(() => {
    state.owned = new Set();
    view.rerender();
  });
  await act(async () => {
    void answer(request, { q: "yes" });
  });
  assert.equal(
    state.sends.length,
    0,
    "revoked ownership published before hydration",
  );
});

test("concurrent questions in one channel retain the newer sending owner", async () => {
  const { state, view, act } = await mount();
  const firstRequest = view.result.current.pending[0];
  const secondEvent = { ...firstRequest.event, id: "7".repeat(64) };
  await act(async () => {
    for (const listener of state.listeners) listener.onEvent(secondEvent);
  });
  const secondRequest = view.result.current.pending.find(
    (item) => item.event.id === secondEvent.id,
  );
  assert.ok(secondRequest);
  let first, second;
  await act(async () => {
    first = view.result.current.answer(firstRequest, { q: "yes" });
    second = view.result.current.answer(secondRequest, { q: "yes" });
  });
  await act(async () => {
    state.sends[0].resolve({ eventId: "first" });
    await first;
  });
  assert.equal(view.result.current.sendingRequestId, secondEvent.id);
  await act(async () => {
    state.sends[1].resolve({ eventId: "second" });
    await second;
  });
});
