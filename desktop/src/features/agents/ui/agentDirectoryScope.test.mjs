import assert from "node:assert/strict";
import { after, afterEach, before, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

const SELF = "1".repeat(64);
const KEY = "a".repeat(64);
const RELAY = "wss://Tenant-A.example";
const raw = {
  pubkey: KEY,
  name: "Scoped agent",
  status: "running",
  backend: { type: "local" },
};
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
  pretendToBeVisual: true,
});
let React,
  act,
  renderHook,
  cleanup,
  waitFor,
  useIdentityQuery,
  QueryClient,
  QueryClientProvider,
  CommunitiesProvider,
  useCommunities,
  useManagedAgentActions;
let calls, handlers, clients, heldSettlers;
let useAgentLifecycleActions,
  useStartManagedAgentMutation,
  useStopManagedAgentMutation,
  toast;
before(async () => {
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    localStorage: dom.window.localStorage,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: dom.window.navigator,
  });
  dom.window.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      calls.push([command, args]);
      if (handlers.has(command)) return handlers.get(command)(args);
      throw new Error(`Unexpected IPC ${command}`);
    },
    transformCallback: () => 1,
  };
  React = await import("react");
  ({ act, renderHook, cleanup, waitFor } = await import(
    "@testing-library/react"
  ));
  ({ QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  ));
  ({ CommunitiesProvider, useCommunities } = await import(
    "../../communities/useCommunities.tsx"
  ));
  ({ useIdentityQuery } = await import("../../../shared/api/hooks.ts"));
  ({ useAgentLifecycleActions } = await import(
    "../../profile/ui/useAgentLifecycleActions.ts"
  ));
  ({ useStartManagedAgentMutation, useStopManagedAgentMutation } = await import(
    "../hooks.ts"
  ));
  ({ toast } = await import("sonner"));
  ({ useManagedAgentActions } = await import("./useManagedAgentActions.ts"));
});
beforeEach(() => {
  calls = [];
  clients = [];
  heldSettlers = [];
  handlers = new Map([
    ["get_identity", () => ({ pubkey: SELF, display_name: "Owner" })],
    ["get_presence", () => ({})],
    [
      "get_channels",
      () => ({ hash: "fixture", channels: [], last_messages: {} }),
    ],
    ["list_managed_agents", () => [raw]],
    ["list_relay_agents", () => []],
    ["start_managed_agent", () => raw],
    ["stop_managed_agent", () => ({ ...raw, status: "stopped" })],
    ["plugin:event|listen", () => 1],
    ["plugin:event|unlisten", () => null],
  ]);
  localStorage.clear();
  localStorage.setItem(
    "buzz-communities",
    JSON.stringify([
      {
        id: "a",
        name: "A",
        relayUrl: RELAY,
        pubkey: SELF,
        addedAt: "2026-01-01",
      },
      {
        id: "b",
        name: "B",
        relayUrl: "wss://tenant-b.example",
        pubkey: SELF,
        addedAt: "2026-01-02",
      },
    ]),
  );
  localStorage.setItem("buzz-active-community-id", "a");
});
afterEach(async () => {
  await act(async () => {
    for (const settle of heldSettlers) settle();
  });
  cleanup();
  for (const client of clients) {
    await client.cancelQueries();
    client.clear();
  }
});
after(() => dom.window.close());
function mount(profile = false, selected = raw) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: Infinity },
      mutations: { retry: false, gcTime: 0 },
    },
  });
  clients.push(client);
  client.setQueryData(["identity"], { pubkey: SELF, displayName: "Owner" });
  client.setQueryData(["managed-agents"], [{ ...raw, personaId: null }]);
  client.setQueryData(["relay-agents"], []);
  client.setQueryData(["channels"], []);
  client.setQueryData(["globalAgentConfig"], { env_vars: {} });
  const wrapper = ({ children }) =>
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(CommunitiesProvider, null, children),
    );
  function Directory() {
    return {
      actions: useManagedAgentActions(),
      community: useCommunities(),
      identity: useIdentityQuery().data,
    };
  }
  function Profile({ selected }) {
    const start = useStartManagedAgentMutation();
    const stop = useStopManagedAgentMutation();
    return {
      actions: useAgentLifecycleActions({
        channels: [],
        managedAgent: selected,
        relayAgents: [],
        startManagedAgent: start.mutateAsync,
        stopManagedAgent: stop.mutateAsync,
      }),
      community: useCommunities(),
      identity: useIdentityQuery().data,
    };
  }
  const hook = renderHook(profile ? Profile : Directory, {
    wrapper,
    initialProps: { selected },
  });
  return { ...hook, client };
}

test("directory Start sends the exact selected key and captured native relay/signer scope", async () => {
  const view = mount();
  await act(() => view.result.current.actions.handleStart(KEY));
  assert.deepEqual(
    calls.find(([command]) => command === "start_managed_agent")?.[1],
    {
      pubkey: KEY,
      expectedRelayUrl: RELAY,
      expectedSignerPubkey: SELF,
    },
  );
});

test("directory Restart cannot start after the community switches while Stop is pending", async () => {
  let release;
  handlers.set(
    "stop_managed_agent",
    () =>
      new Promise((resolve) => {
        release = resolve;
        heldSettlers.push(() => resolve(raw));
      }),
  );
  const view = mount();
  let operation;
  await act(async () => {
    operation = view.result.current.actions.handleRestart(KEY);
  });
  assert.equal(
    typeof release,
    "function",
    "real stop command reached the held boundary",
  );
  await act(async () => {
    view.result.current.community.switchCommunity("b");
  });
  await act(async () => {
    release({ ...raw, status: "stopped" });
    await operation;
  });
  assert.equal(
    calls.filter(([command]) => command === "start_managed_agent").length,
    0,
  );
  assert.equal(
    view.result.current.actions.actionErrorMessage,
    null,
    "old action must not report into the new community",
  );
});

test("directory Start completion cannot report an old error after identity replacement", async () => {
  let rejectStart;
  handlers.set(
    "start_managed_agent",
    () =>
      new Promise((_resolve, reject) => {
        rejectStart = reject;
        heldSettlers.push(() => reject(new Error("test cleanup")));
      }),
  );
  const view = mount();
  let operation;
  await act(async () => {
    operation = view.result.current.actions.handleStart(KEY);
  });
  assert.equal(typeof rejectStart, "function");
  await act(async () => {
    view.client.setQueryData(["identity"], {
      pubkey: "2".repeat(64),
      displayName: "New owner",
    });
  });
  await waitFor(() =>
    assert.equal(view.result.current.identity?.pubkey, "2".repeat(64)),
  );
  await act(async () => {
    rejectStart(new Error("old owner spawn failure"));
    await operation;
  });
  assert.equal(view.result.current.actions.actionErrorMessage, null);
});

for (const change of ["round trip", "unmount", "removed target"]) {
  test(`pending directory Restart is invalidated by ${change}`, async () => {
    let release;
    handlers.set(
      "stop_managed_agent",
      () =>
        new Promise((resolve) => {
          release = resolve;
          heldSettlers.push(() => resolve(raw));
        }),
    );
    const view = mount();
    let operation;
    await act(async () => {
      operation = view.result.current.actions.handleRestart(KEY);
    });
    assert.equal(typeof release, "function");
    if (change === "round trip") {
      await act(async () => {
        view.result.current.community.switchCommunity("b");
      });
      await act(async () => {
        view.result.current.community.switchCommunity("a");
      });
    } else if (change === "unmount") view.unmount();
    else {
      handlers.set("list_managed_agents", () => []);
      await act(async () => {
        view.client.setQueryData(["managed-agents"], []);
      });
      await waitFor(() =>
        assert.equal(view.result.current.actions.managedAgents.length, 0),
      );
    }
    await act(async () => {
      release({ ...raw, status: "stopped" });
      await operation;
    });
    assert.equal(
      calls.filter(([command]) => command === "start_managed_agent").length,
      0,
    );
    if (change !== "unmount")
      assert.equal(
        view.result.current.actions.restartingAgentPubkey,
        null,
        "retired restart cannot block a new action",
      );
  });
}

test("scope-equivalent identity refresh preserves pending Start state for the selected key", async () => {
  let resolveStart;
  handlers.set(
    "start_managed_agent",
    () =>
      new Promise((resolve) => {
        resolveStart = resolve;
        heldSettlers.push(() => resolve(raw));
      }),
  );
  const view = mount();
  let operation;
  await act(async () => {
    operation = view.result.current.actions.handleStart(KEY);
  });
  await waitFor(() =>
    assert.equal(view.result.current.actions.startingAgentPubkey, KEY),
  );
  await act(async () => {
    view.client.setQueryData(["identity"], {
      pubkey: SELF,
      displayName: "Renamed owner",
    });
  });
  await waitFor(() =>
    assert.equal(view.result.current.actions.startingAgentPubkey, KEY),
  );
  await act(async () => {
    resolveStart(raw);
    await operation;
  });
  assert.equal(view.result.current.actions.actionErrorMessage, null);
});

test("directory Start refuses a missing scope before native invocation", async () => {
  localStorage.clear();
  const view = mount();
  await act(() => view.result.current.actions.handleStart(KEY));
  assert.equal(
    calls.filter(([command]) => command === "start_managed_agent").length,
    0,
  );
  assert.match(
    view.result.current.actions.actionErrorMessage,
    /community|identity/i,
  );
});

test("profile Start sends native scope with the exact displayed instance", async () => {
  const view = mount(true, { ...raw, status: "stopped" });
  await act(() => view.result.current.actions.handleAgentPrimaryAction());
  assert.deepEqual(
    calls.find(([command]) => command === "start_managed_agent")?.[1],
    {
      pubkey: KEY,
      expectedRelayUrl: RELAY,
      expectedSignerPubkey: SELF,
    },
  );
});

test("profile replacement during Restart cannot start the retired target or report success", async () => {
  let release;
  handlers.set(
    "stop_managed_agent",
    () =>
      new Promise((resolve) => {
        release = resolve;
        heldSettlers.push(() => resolve(raw));
      }),
  );
  const priorToasts = new Set(toast.getToasts().map((entry) => entry.id));
  const view = mount(true);
  let operation;
  await act(async () => {
    operation = view.result.current.actions.handleAgentRestart();
  });
  view.rerender({
    selected: { ...raw, pubkey: "b".repeat(64), name: "Another agent" },
  });
  await act(async () => {
    release({ ...raw, status: "stopped" });
    await operation;
  });
  assert.equal(
    calls.filter(([command]) => command === "start_managed_agent").length,
    0,
  );
  assert.deepEqual(
    toast.getToasts().filter((entry) => !priorToasts.has(entry.id)),
    [],
  );
});
