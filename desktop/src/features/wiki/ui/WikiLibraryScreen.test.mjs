/**
 * Rendered coverage for the Wiki library detail regeneration affordance (R4).
 *
 * The gate only exists once the REAL WikiLibraryScreen wires the REAL
 * useWikiEventsQuery / useWikiPublicationRecovery hooks to the REAL
 * WikiPageView + WikiHeaderControls, which is why an assertion on the
 * predicate alone cannot prove it. Regeneration captures fresh source, so a
 * repository with no linked local workspace must not offer it — while
 * Reconcile, which is read-only, must stay available and operable.
 *
 * Only the app navigation boundary is stubbed. Everything else — the screen,
 * the detail view, the header controls, the recovery hook and its native IPC
 * contract — runs its production code.
 */

import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, test } from "node:test";
import { JSDOM } from "jsdom";

// useAppNavigation reaches the router/shell, which has nothing to do with the
// recovery affordance and drags the whole app shell into the mount.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/app/navigation/useAppNavigation") {
      return { shortCircuit: true, url: "buzz-wiki-stub:useAppNavigation" };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url === "buzz-wiki-stub:useAppNavigation") {
      return {
        format: "module",
        shortCircuit: true,
        source:
          "export const useAppNavigation = () => ({ goProject: () => {} });\n",
      };
    }
    return nextLoad(url, context);
  },
});

const OWNER = "a".repeat(64);
const COMMUNITY = "https://relay.example";
const REPO_D = "crew";
const LINKED_PATH = "/Users/tester/src/crew";
const RETIRED_DEPENDENCY = "e".repeat(64);

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

class NoopObserver {
  disconnect() {}
  observe() {}
  unobserve() {}
}

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
  ResizeObserver: NoopObserver,
  IntersectionObserver: NoopObserver,
});

function scope() {
  return {
    scope: { owner: OWNER, community: COMMUNITY },
    workspace_generation: 1,
    identity_generation: 1,
  };
}

function repository(localWorkspacePath) {
  return {
    id: REPO_D,
    dtag: REPO_D,
    name: REPO_D,
    description: "",
    cloneUrls: [],
    localWorkspacePath,
    localWorkspaceStatus: localWorkspacePath ? "linked" : "unlinked",
    workspaceMode: "git",
    webUrl: null,
    owner: OWNER,
    contributors: [],
    createdAt: 1,
    status: "open",
    defaultBranch: "main",
    repoAddress: `30617:${OWNER}:${REPO_D}`,
    channelId: null,
  };
}

/**
 * A durable native row proving a retired immutable dependency: unresolved,
 * read-only, and therefore Regenerate-only.
 */
function retiredJob() {
  return {
    id: "operation-1",
    revision: 3,
    resourceKey: `30617:${OWNER}:${REPO_D}`,
    status: "reconciling",
    reconciled: false,
    snapshotId: "snapshot-1",
    sourceRevision: `git:${"b".repeat(40)}`,
    cadence: "manual",
    pages: 2,
    attempts: 1,
    progress: "pages",
    headAttempted: true,
    cancelRequested: false,
    reconcileOnly: true,
    retiredDependencyId: RETIRED_DEPENDENCY,
    retryAt: 0,
    lastError: "Wiki immutable dependency retired.",
  };
}

function installTauriInvoke(handler) {
  const bridge = { invoke: handler, transformCallback: () => 1 };
  globalThis.__TAURI_INTERNALS__ = bridge;
  dom.window.__TAURI_INTERNALS__ = bridge;
}

const { act, cleanup, render, screen, waitFor, fireEvent } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { relayClient } = await import("@/shared/api/relayClient");
const { resetWikiStore } = await import("@/features/wiki/lib/wikiStore");
const { WikiLibraryScreen } = await import(
  "@/features/wiki/ui/WikiLibraryScreen"
);

after(() => dom.window.close());

/**
 * Mount the real screen with one repository and one retired native row, then
 * navigate into its detail view. WikiRepoCard's onOpen is unconditional, so
 * the detail view is reachable without any real Wiki graph.
 */
async function mountDetail(localWorkspacePath, calls) {
  const expected = scope();
  const originalFetchEvents = relayClient.fetchEvents;
  relayClient.fetchEvents = async () => [];
  installTauriInvoke(async (command, args) => {
    calls.push({ command, args });
    if (command === "owner_operation_scope") return expected;
    if (command === "wiki_snapshot_read") {
      return {
        token: expected,
        value: {
          state: "missing",
          head: null,
          manifest: null,
          pages: [],
          repoState: null,
        },
      };
    }
    if (command === "wiki_publication_list") {
      return { token: expected, value: [retiredJob()] };
    }
    if (command === "wiki_publication_regenerate") {
      // The captured arguments are the contract under test; refusing here
      // keeps the test off the dispatch path entirely.
      throw new Error("fixture refusal after capture");
    }
    if (command === "wiki_publication_reconcile") {
      return { token: expected, value: retiredJob() };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  });

  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  // useProjectsQuery has a positive staleTime, so seeding its cache prevents
  // any repository fetch.
  client.setQueryData(
    ["projects"],
    [{ id: "project-1", repositories: [repository(localWorkspacePath)] }],
  );

  const view = render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(WikiLibraryScreen),
    ),
  );

  const card = await screen.findByTestId(`wiki-repo-card-${REPO_D}`);
  await act(async () => {
    fireEvent.click(card.querySelector("button"));
  });
  await waitFor(() => {
    assert.ok(
      screen.queryByTestId("wiki-recovery-header"),
      "the detail view must render the durable recovery row",
    );
  });

  return {
    client,
    dispose() {
      view.unmount();
      client.clear();
      cleanup();
      resetWikiStore();
      relayClient.fetchEvents = originalFetchEvents;
      delete globalThis.__TAURI_INTERNALS__;
      delete dom.window.__TAURI_INTERNALS__;
    },
  };
}

function buttonNamed(text) {
  return screen
    .queryAllByRole("button")
    .find((node) => node.textContent?.trim() === text);
}

test("a retired row with no linked workspace hides Regenerate but keeps Reconcile", async () => {
  const calls = [];
  const mounted = await mountDetail(null, calls);
  try {
    assert.equal(
      screen.queryByTestId("wiki-regenerate-recovery"),
      null,
      "Regenerate needs local source and must not be offered without it",
    );
    const reconcile = buttonNamed("Reconcile");
    assert.ok(reconcile, "read-only Reconcile stays available");
    assert.equal(reconcile.disabled, false, "and stays operable");

    await act(async () => {
      fireEvent.click(reconcile);
    });
    await waitFor(() => {
      assert.ok(
        calls.some((call) => call.command === "wiki_publication_reconcile"),
        "Reconcile must reach the native read-only command",
      );
    });
    assert.ok(
      !calls.some((call) => call.command === "wiki_publication_regenerate"),
      "no regeneration may be attempted without a linked workspace",
    );
  } finally {
    mounted.dispose();
  }
});

test("a linked workspace exposes Regenerate and forwards the captured path", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls);
  try {
    const regenerate = await screen.findByTestId("wiki-regenerate-recovery");
    assert.ok(buttonNamed("Reconcile"), "Reconcile remains available too");

    await act(async () => {
      fireEvent.click(regenerate);
    });
    await waitFor(() => {
      assert.ok(
        calls.some((call) => call.command === "wiki_publication_regenerate"),
        "Regenerate must reach the native successor command",
      );
    });
    const call = calls.find(
      (entry) => entry.command === "wiki_publication_regenerate",
    );
    assert.equal(
      call.args.repoPath,
      LINKED_PATH,
      "the exact captured local path is forwarded",
    );
    assert.equal(call.args.id, "operation-1");
    assert.equal(call.args.revision, 3);
  } finally {
    mounted.dispose();
  }
});
