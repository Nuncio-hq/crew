/**
 * A delete that FAILS leaves a durable cleanup record behind, and the recovery
 * banner's query does not refetch on window focus and stays fresh for 30s. If
 * the delete mutations do not invalidate it, the owner reads "Failed to delete
 * agent." with no retry affordance on screen. The failure path is the one that
 * matters, so that is the path asserted here — against the production hooks.
 */

import assert from "node:assert/strict";
import { after, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});

const { cleanup, renderHook, waitFor } = await import("@testing-library/react");
const React = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const {
  managedAgentDeletionsQueryKey,
  useDeleteManagedAgentMutation,
  useDeletePersonaMutation,
} = await import("./hooks.ts");

after(() => dom.window.close());

function installFailingInvoke() {
  const bridge = {
    invoke: async (command) => {
      if (command === "delete_persona" || command === "delete_managed_agent") {
        throw new Error("native deletion failed");
      }
      return null;
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  dom.window.__TAURI_INTERNALS__ = bridge;
}

function mount(useMutationHook) {
  const client = new QueryClient({
    // gcTime defaults to five minutes; a cached entry would keep a timer (and
    // this test process) alive long after the assertions finish.
    defaultOptions: {
      mutations: { gcTime: 0, retry: false },
      queries: { gcTime: 0, retry: false },
    },
  });
  const invalidated = [];
  const realInvalidate = client.invalidateQueries.bind(client);
  client.invalidateQueries = (filters) => {
    invalidated.push(filters?.queryKey);
    return realInvalidate(filters);
  };
  const view = renderHook(() => useMutationHook(), {
    wrapper: ({ children }) =>
      React.createElement(QueryClientProvider, { client }, children),
  });
  return { client, invalidated, view };
}

function sawDeletionsKey(invalidated) {
  return invalidated.some(
    (key) => Array.isArray(key) && key[0] === managedAgentDeletionsQueryKey[0],
  );
}

test("a failed persona delete refreshes the deletion recovery list", async (t) => {
  installFailingInvoke();
  const { client, invalidated, view } = mount(useDeletePersonaMutation);
  t.after(() => {
    cleanup();
    client.unmount();
    client.clear();
  });

  await assert.rejects(() => view.result.current.mutateAsync("persona-1"));
  await waitFor(() =>
    assert.ok(
      sawDeletionsKey(invalidated),
      "the recovery banner query must be invalidated after a failed delete",
    ),
  );
});

test("a failed instance delete refreshes the deletion recovery list", async (t) => {
  installFailingInvoke();
  const { client, invalidated, view } = mount(useDeleteManagedAgentMutation);
  t.after(() => {
    cleanup();
    client.unmount();
    client.clear();
  });

  await assert.rejects(() =>
    view.result.current.mutateAsync({ pubkey: "a".repeat(64) }),
  );
  await waitFor(() =>
    assert.ok(
      sawDeletionsKey(invalidated),
      "the recovery banner query must be invalidated after a failed delete",
    ),
  );
});
