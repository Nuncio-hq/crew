import assert from "node:assert/strict";
import { after, afterEach, before, beforeEach, mock, test } from "node:test";
import { JSDOM } from "jsdom";

const SELF = "1".repeat(64);
const KEY = "a".repeat(64);
const RELAY = "wss://message-a.example";
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
  pretendToBeVisual: true,
});
Object.assign(globalThis, {
  window: dom.window,
  self: dom.window,
  document: dom.window.document,
  localStorage: dom.window.localStorage,
  HTMLElement: dom.window.HTMLElement,
  MutationObserver: dom.window.MutationObserver,
  IS_REACT_ACT_ENVIRONMENT: true,
});
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: dom.window.navigator,
});
Object.defineProperty(navigator, "mediaDevices", {
  configurable: true,
  value: {
    addEventListener() {},
    removeEventListener() {},
    enumerateDevices: async () => [],
  },
});
globalThis.requestAnimationFrame = dom.window.requestAnimationFrame.bind(
  dom.window,
);
globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__ =
  dom.window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
let calls,
  releaseDm,
  client,
  view,
  closed,
  current,
  setTarget,
  pendingDms,
  errors;
let toast;
const rawDm = {
  id: "fixture-dm",
  name: "Agent DM",
  channel_type: "dm",
  visibility: "private",
  is_member: true,
  member_count: 2,
  member_pubkeys: [SELF, KEY],
  participant_pubkeys: [SELF, KEY],
  participants: [],
  archived_at: null,
  last_message_at: null,
  description: "",
  purpose: null,
  topic: null,
  ttl_deadline: null,
  ttl_seconds: null,
};
globalThis.__TAURI_INTERNALS__ = dom.window.__TAURI_INTERNALS__ = {
  invoke: async (command, args) => {
    if (command.startsWith("plugin:event|")) return 0;
    if (command === "get_identity")
      return { pubkey: SELF, display_name: "Owner" };
    if (command === "get_channels")
      return { hash: "fixture", channels: [], last_messages: {} };
    if (command === "open_dm") {
      calls.push(args);
      return new Promise((resolve, reject) => {
        releaseDm = () => resolve(rawDm);
        pendingDms.push({ resolve: releaseDm, reject });
      });
    }
    throw new Error(`unmocked IPC: ${command}`);
  },
  transformCallback: () => 1,
};
let useIdentityQuery;
let React,
  act,
  render,
  cleanup,
  waitFor,
  QueryClient,
  QueryClientProvider,
  CommunitiesProvider,
  useCommunities,
  HuddleProvider,
  useProfileInteractionActions,
  createRootRoute,
  createRoute,
  createRouter,
  createMemoryHistory,
  RouterProvider;
before(async () => {
  React = await import("react");
  ({ toast } = await import("sonner"));
  ({ act, render, cleanup, waitFor } = await import("@testing-library/react"));
  ({ QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  ));
  ({ CommunitiesProvider, useCommunities } = await import(
    "../../communities/useCommunities.tsx"
  ));
  ({ HuddleProvider } = await import("../../huddle/index.ts"));
  ({ useIdentityQuery } = await import("../../../shared/api/hooks.ts"));
  ({ useProfileInteractionActions } = await import(
    "./useProfileInteractionActions.ts"
  ));
  ({
    createRootRoute,
    createRoute,
    createRouter,
    createMemoryHistory,
    RouterProvider,
  } = await import("@tanstack/react-router"));
});
beforeEach(() => {
  calls = [];
  pendingDms = [];
  errors = [];
  mock.method(toast, "error", (error) => errors.push(error));
  closed = 0;
  releaseDm = null;
  client = null;
  localStorage.clear();
  localStorage.setItem(
    "buzz-communities",
    JSON.stringify([
      { id: "a", name: "A", relayUrl: RELAY, addedAt: "2026-01-01" },
      {
        id: "b",
        name: "B",
        relayUrl: "wss://message-b.example",
        addedAt: "2026-01-02",
      },
    ]),
  );
  localStorage.setItem("buzz-active-community-id", "a");
});
afterEach(async () => {
  await act(async () => {
    for (const pending of pendingDms) pending.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  mock.restoreAll();
  cleanup();
  await client?.cancelQueries();
  client?.clear();
});
after(() => dom.window.close());
async function mount() {
  client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: Infinity },
      mutations: { retry: false, gcTime: 0 },
    },
  });
  client.setQueryData(["identity"], { pubkey: SELF, displayName: "Owner" });
  client.setQueryData(["channels"], []);
  function Surface() {
    const [target, update] = React.useState(KEY);
    setTarget = update;
    current = {
      actions: useProfileInteractionActions({
        effectivePubkey: target,
        enabled: true,
        isBot: true,
        isSelf: false,
        viewerIsOwner: true,
        onClose: () => {
          closed += 1;
        },
      }),
      community: useCommunities(),
      identity: useIdentityQuery().data,
    };
    return null;
  }
  const rootRoute = createRootRoute({ component: Surface });
  const channelRoute = createRoute({
    getParentRoute: () => rootRoute,
    path: "/channels/$channelId",
    component: () => null,
  });
  const router = createRouter({
    routeTree: rootRoute.addChildren([channelRoute]),
    history: createMemoryHistory({ initialEntries: ["/"] }),
  });
  await router.load();
  await act(async () => {
    view = render(
      React.createElement(
        QueryClientProvider,
        { client },
        React.createElement(
          CommunitiesProvider,
          null,
          React.createElement(
            HuddleProvider,
            { ownsAudioSession: false },
            React.createElement(RouterProvider, { router }),
          ),
        ),
      ),
    );
  });
  await waitFor(() => assert.ok(current));
  return router;
}
async function startMessage() {
  await act(async () => {
    current.actions.handleMessage();
  });
  await waitFor(() => assert.equal(typeof releaseDm, "function"));
}

test("profile Message invokes native open_dm with captured scope and exact identity", async () => {
  await mount();
  await startMessage();
  assert.deepEqual(calls, [
    { pubkeys: [KEY], expectedRelayUrl: RELAY, expectedSignerPubkey: SELF },
  ]);
});

for (const change of ["target", "community", "identity", "unmount"]) {
  test(`held profile Message cannot navigate or close after ${change} changes`, async () => {
    const router = await mount();
    await startMessage();
    await act(async () => {
      if (change === "target") setTarget("b".repeat(64));
      else if (change === "community") current.community.switchCommunity("b");
      else if (change === "identity")
        client.setQueryData(["identity"], {
          pubkey: "2".repeat(64),
          displayName: "New owner",
        });
      else view.unmount();
    });
    if (change === "identity")
      await waitFor(() =>
        assert.equal(current.identity?.pubkey, "2".repeat(64)),
      );
    await act(async () => {
      releaseDm();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    assert.equal(router.state.location.pathname, "/");
    assert.equal(closed, 0);
  });
}

for (const change of ["target", "community", "identity"]) {
  for (const outcome of ["resolve", "reject"]) {
    test(`Message ${change} switch permits independent pending action and fences late ${outcome}`, async () => {
      const router = await mount();
      await startMessage();
      assert.equal(current.actions.pendingAction, "message");
      await act(async () => {
        if (change === "target") setTarget("b".repeat(64));
        else if (change === "community") current.community.switchCommunity("b");
        else
          client.setQueryData(["identity"], {
            pubkey: "2".repeat(64),
            displayName: "New owner",
          });
      });
      await waitFor(() => assert.equal(current.actions.pendingAction, null));
      await act(async () => current.actions.handleMessage());
      await waitFor(() => assert.equal(calls.length, 2));
      assert.equal(current.actions.pendingAction, "message");
      assert.deepEqual(calls[1], {
        pubkeys: [change === "target" ? "b".repeat(64) : KEY],
        expectedRelayUrl:
          change === "community" ? "wss://message-b.example" : RELAY,
        expectedSignerPubkey: change === "identity" ? "2".repeat(64) : SELF,
      });
      await act(async () => {
        if (outcome === "resolve") pendingDms[0].resolve();
        else pendingDms[0].reject(new Error("Retired Message failed"));
        await new Promise((resolve) => setTimeout(resolve, 0));
      });
      assert.equal(current.actions.pendingAction, "message");
      assert.equal(router.state.location.pathname, "/");
      assert.equal(closed, 0);
      assert.deepEqual(errors, []);
      await act(async () => {
        pendingDms[1].resolve();
        await new Promise((resolve) => setTimeout(resolve, 0));
      });
      await waitFor(() => assert.equal(current.actions.pendingAction, null));
      assert.equal(router.state.location.pathname, "/channels/fixture-dm");
      assert.equal(closed, 1);
    });
  }
}
