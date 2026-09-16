/**
 * Rendered coverage for what the deletion recovery banner tells the owner.
 *
 * A persona cleanup's resource key is a synthetic digest, not an agent public
 * key, so the old "<12 hex chars>… keeps its agent stopped" row named nothing
 * the owner could recognise. The banner, its query hook, and the native IPC
 * mapping all run their production code here; only the communities context is
 * stubbed, because it reaches the app shell.
 */

import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, test } from "node:test";
import { JSDOM } from "jsdom";

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/features/communities/useCommunities") {
      return { shortCircuit: true, url: "buzz-agents-stub:useCommunities" };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url === "buzz-agents-stub:useCommunities") {
      return {
        format: "module",
        shortCircuit: true,
        source:
          "export const useCommunities = () => ({ communities: [{ id: 'local', relayUrl: 'wss://relay.example' }], activeCommunity: { id: 'local', relayUrl: 'wss://relay.example' }, switchCommunity: () => {} });\n",
      };
    }
    return nextLoad(url, context);
  },
});

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  Node: dom.window.Node,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});

const { cleanup, render, screen, waitFor } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { ManagedAgentDeletionRecoveryBanner } = await import(
  "./ManagedAgentDeletionRecoveryBanner.tsx"
);

after(() => dom.window.close());

const AGENT_KEY = "a".repeat(64);
const PERSONA_KEY = "f".repeat(64);

function installTauriInvoke(rows) {
  const bridge = {
    invoke: async (command) => {
      if (command === "list_managed_agent_deletions") {
        return rows;
      }
      return null;
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  dom.window.__TAURI_INTERNALS__ = bridge;
}

function nativeSummary(overrides) {
  return {
    id: "operation-1",
    owner: "b".repeat(64),
    community: "https://relay.example",
    resource_key: AGENT_KEY,
    target_kind: "agent",
    revision: 1,
    status: "failed",
    reconciled: false,
    updated_at: 10,
    ...overrides,
  };
}

const clients = [];

after(() => {
  for (const client of clients) {
    client.unmount();
    client.clear();
  }
});

async function renderBanner(rows) {
  installTauriInvoke(rows);
  const client = new QueryClient({
    // gcTime defaults to five minutes; a cached entry would keep a timer (and
    // this test process) alive long after the assertions finish.
    defaultOptions: { queries: { gcTime: 0, retry: false } },
  });
  clients.push(client);
  render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(ManagedAgentDeletionRecoveryBanner),
    ),
  );
  await waitFor(() =>
    assert.ok(screen.getByTestId("managed-agent-deletion-recovery")),
  );
}

test("a persona coordinator is named as a persona cleanup, not a key slice", async (t) => {
  t.after(cleanup);
  await renderBanner([
    nativeSummary({
      id: "persona-operation",
      resource_key: PERSONA_KEY,
      target_kind: "persona",
    }),
  ]);

  const row = screen.getByTestId("managed-agent-deletion-recovery").textContent;
  assert.match(row, /Persona cleanup/);
  assert.ok(
    !row.includes(PERSONA_KEY.slice(0, 12)),
    "a synthetic coordinator digest must not be shown as an agent key",
  );
});

test("an instance deletion still shows its agent key", async (t) => {
  t.after(cleanup);
  await renderBanner([nativeSummary({})]);

  const row = screen.getByTestId("managed-agent-deletion-recovery").textContent;
  assert.match(row, new RegExp(`Agent ${AGENT_KEY.slice(0, 12)}`));
});

test("a native build without the discriminator degrades to the agent wording", async (t) => {
  t.after(cleanup);
  await renderBanner([nativeSummary({ target_kind: undefined })]);

  const row = screen.getByTestId("managed-agent-deletion-recovery").textContent;
  assert.match(row, new RegExp(`Agent ${AGENT_KEY.slice(0, 12)}`));
});
