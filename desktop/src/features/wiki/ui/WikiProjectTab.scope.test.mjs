import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, afterEach, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/features/wiki/ui/WikiAskBox") {
      return { shortCircuit: true, url: "crew-wiki-scope-stub:ask" };
    }
    if (specifier === "@/features/wiki/ui/WikiSourceFiles") {
      return { shortCircuit: true, url: "crew-wiki-scope-stub:source" };
    }
    if (specifier === "@/app/navigation/useAppNavigation") {
      return { shortCircuit: true, url: "crew-wiki-scope-stub:navigation" };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url === "crew-wiki-scope-stub:ask") {
      return {
        format: "module",
        shortCircuit: true,
        source: "export const WikiAskBox = () => null;\n",
      };
    }
    if (url === "crew-wiki-scope-stub:source") {
      return {
        format: "module",
        shortCircuit: true,
        source: "export const WikiSourceFiles = () => null;\n",
      };
    }
    if (url === "crew-wiki-scope-stub:navigation") {
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
const COORDINATE = `30617:${OWNER}:${REPO_D}`;

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  localStorage: dom.window.localStorage,
  window: dom.window,
  ResizeObserver: class {
    disconnect() {}
    observe() {}
    unobserve() {}
  },
  IntersectionObserver: class {
    disconnect() {}
    observe() {}
    unobserve() {}
  },
});

const { act, cleanup, fireEvent, render, screen, waitFor } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { relayClient } = await import("@/shared/api/relayClient");
const { clearWikiReadCoordinators } = await import(
  "@/features/wiki/hooks/wikiReadCoordinator"
);
const { resetWikiStore } = await import("@/features/wiki/lib/wikiStore");
const { WikiProjectTab } = await import("./WikiProjectTab.tsx");

const scope = {
  scope: { owner: OWNER, community: COMMUNITY },
  workspace_generation: 1,
  identity_generation: 1,
};

function repository() {
  return {
    id: REPO_D,
    dtag: REPO_D,
    name: "Crew",
    description: "",
    cloneUrls: [],
    localWorkspacePath: null,
    localWorkspaceStatus: "unlinked",
    workspaceMode: "git",
    webUrl: null,
    owner: OWNER,
    contributors: [],
    createdAt: 1,
    status: "open",
    defaultBranch: "main",
    repoAddress: COORDINATE,
    channelId: null,
  };
}

function missingSnapshot() {
  return {
    state: "missing",
    head: null,
    manifest: null,
    pages: [],
    repoState: null,
  };
}

const SNAPSHOT_ID = "1".repeat(64);
const COMMIT = "c".repeat(40);

function event(id, content, tags) {
  return {
    id,
    pubkey: OWNER,
    kind: 30623,
    content,
    created_at: 10,
    sig: "s".repeat(128),
    tags,
  };
}

function completeSnapshot() {
  const head = event(
    "2".repeat(64),
    JSON.stringify({
      sections: [
        {
          id: "overview",
          title: "Overview",
          pages: [{ slug: "intro", title: "Introduction" }],
        },
      ],
    }),
    [
      ["d", `${REPO_D}/_toc`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT_ID],
      ["commit", COMMIT],
      ["branch", "main"],
    ],
  );
  const page = event("3".repeat(64), "Accepted Wiki article body", [
    ["d", `${REPO_D}/intro`],
    ["a", COORDINATE],
    ["wiki-version", "1"],
    ["wiki-snapshot", SNAPSHOT_ID],
    ["title", "Introduction"],
    ["section", "overview"],
    ["commit", COMMIT],
    ["language", "en"],
  ]);
  const manifest = event(
    "4".repeat(64),
    JSON.stringify([
      1,
      SNAPSHOT_ID,
      OWNER,
      REPO_D,
      `git:${COMMIT}`,
      null,
      [],
      [],
    ]),
    [
      ["d", `${REPO_D}/manifest`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT_ID],
    ],
  );
  return {
    state: "complete",
    head,
    manifest,
    pages: [page],
    repoState: null,
  };
}

let mode;
let calls;
let releaseScope;
let releaseSnapshot;
let originalFetchEvents;

function installBridge() {
  calls = [];
  mode = {
    scope: "ready",
    snapshot: "ready",
  };
  globalThis.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "owner_operation_scope") {
        if (mode.scope === "pending") {
          await new Promise((resolve) => {
            releaseScope = resolve;
          });
        }
        if (mode.scope === "error") {
          throw new Error("owner scope unavailable");
        }
        return scope;
      }
      if (command === "wiki_snapshot_read") {
        if (mode.snapshot === "pending") {
          await new Promise((resolve) => {
            releaseSnapshot = resolve;
          });
        }
        if (mode.snapshot === "error") {
          throw new Error("repository read unavailable");
        }
        return {
          token: scope,
          value:
            mode.snapshot === "complete"
              ? completeSnapshot()
              : missingSnapshot(),
        };
      }
      if (command === "wiki_publication_list") {
        return { token: scope, value: [] };
      }
      throw new Error(`Unexpected Tauri command: ${command}`);
    },
    transformCallback: () => 1,
  };
  dom.window.__TAURI_INTERNALS__ = globalThis.__TAURI_INTERNALS__;
}

function mount() {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { gcTime: 0 },
    },
  });
  client.setQueryData(
    ["projects"],
    [{ id: "project-1", repositories: [repository()] }],
  );
  const view = render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(WikiProjectTab, {
        project: repository(),
        projectId: "project-1",
      }),
    ),
  );
  return {
    client,
    dispose() {
      view.unmount();
      client.clear();
      client.unmount();
      cleanup();
      clearWikiReadCoordinators();
      resetWikiStore();
      delete globalThis.__TAURI_INTERNALS__;
      delete dom.window.__TAURI_INTERNALS__;
    },
  };
}

beforeEach(() => {
  cleanup();
  dom.window.localStorage.clear();
  installBridge();
  originalFetchEvents = relayClient.fetchEvents;
  relayClient.fetchEvents = async () => [];
});

afterEach(() => {
  relayClient.fetchEvents = originalFetchEvents;
  cleanup();
  clearWikiReadCoordinators();
  resetWikiStore();
  delete globalThis.__TAURI_INTERNALS__;
  delete dom.window.__TAURI_INTERNALS__;
});

after(() => {
  cleanup();
  dom.window.close();
});

test("Project Wiki stays truthful through deferred scope and repository success", async () => {
  mode.scope = "pending";
  mode.snapshot = "pending";
  const mounted = mount();
  try {
    await waitFor(() => screen.getByTestId("wiki-read-loading"));
    assert.equal(screen.queryByText("Never generated"), null);

    await act(async () => {
      mode.scope = "ready";
      releaseScope();
    });
    await waitFor(() => {
      assert.ok(
        calls.some(({ command }) => command === "wiki_snapshot_read"),
        "a ready owner scope must start the repository read",
      );
    });
    assert.ok(screen.getByTestId("wiki-read-loading"));
    assert.equal(screen.queryByText("Never generated"), null);

    await act(async () => {
      mode.snapshot = "ready";
      releaseSnapshot();
    });
    await waitFor(() => screen.getByText("Never generated"));
    assert.equal(screen.queryByTestId("wiki-read-loading"), null);
  } finally {
    mounted.dispose();
  }
});

test("Project Wiki exposes scope failure and retries the failed capture", async () => {
  mode.scope = "error";
  const mounted = mount();
  try {
    await waitFor(() => screen.getByTestId("wiki-read-unavailable"));
    assert.match(
      screen.getByTestId("wiki-read-unavailable").textContent ?? "",
      /owner scope unavailable/,
    );
    const retry = screen.getByTestId("wiki-retry-read");

    mode.scope = "pending";
    await act(async () => {
      fireEvent.click(retry);
    });
    await waitFor(() => screen.getByTestId("wiki-read-loading"));
    assert.equal(screen.queryByText("Never generated"), null);

    await act(async () => {
      mode.scope = "ready";
      releaseScope();
    });
    await waitFor(() => screen.getByText("Never generated"));
    assert.ok(
      calls.filter(({ command }) => command === "owner_operation_scope")
        .length >= 2,
      "retry must recapture the owner scope",
    );
  } finally {
    mounted.dispose();
  }
});

test("Project Wiki exposes a repository read failure and retries that read", async () => {
  mode.snapshot = "error";
  const mounted = mount();
  try {
    await waitFor(() => screen.getByTestId("wiki-read-unavailable"));
    assert.match(
      screen.getByTestId("wiki-read-unavailable").textContent ?? "",
      /repository read unavailable/,
    );
    assert.equal(screen.queryByText("Never generated"), null);

    mode.snapshot = "pending";
    await act(async () => {
      fireEvent.click(screen.getByTestId("wiki-retry-read"));
    });
    await waitFor(() => screen.getByTestId("wiki-read-loading"));
    assert.equal(screen.queryByText("Never generated"), null);

    await act(async () => {
      mode.snapshot = "ready";
      releaseSnapshot();
    });
    await waitFor(() => screen.getByText("Never generated"));
  } finally {
    mounted.dispose();
  }
});

test("Project Wiki does not gate an accepted repository read on a company read", async () => {
  let releaseCompany;
  const companyRead = new Promise((resolve) => {
    releaseCompany = resolve;
  });
  relayClient.fetchEvents = async () => {
    await companyRead;
    return [];
  };
  const mounted = mount();
  try {
    await waitFor(() => screen.getByText("Never generated"));
    assert.equal(screen.queryByTestId("wiki-read-loading"), null);
    assert.equal(screen.queryByTestId("wiki-read-unavailable"), null);
  } finally {
    await act(async () => releaseCompany());
    mounted.dispose();
  }
});

test("Project Wiki keeps an accepted repository read when the company read fails", async () => {
  relayClient.fetchEvents = async () => {
    throw new Error("company Wiki unavailable");
  };
  const mounted = mount();
  try {
    await waitFor(() => screen.getByText("Never generated"));
    assert.equal(screen.queryByTestId("wiki-read-loading"), null);
    assert.equal(screen.queryByTestId("wiki-read-unavailable"), null);
  } finally {
    mounted.dispose();
  }
});

test("Project Wiki keeps accepted article content through company failure, then revokes it on scope loss", async () => {
  mode.snapshot = "complete";
  relayClient.fetchEvents = async () => {
    throw new Error("company Wiki unavailable");
  };
  const mounted = mount();
  try {
    await waitFor(() => screen.getByText("Accepted Wiki article body"));
    assert.equal(screen.queryByTestId("wiki-read-loading"), null);
    assert.equal(screen.queryByTestId("wiki-read-unavailable"), null);
    assert.notEqual(
      screen.getByTestId("wiki-freshness").textContent,
      "Never generated",
    );

    mode.scope = "error";
    await act(async () => {
      await mounted.client.invalidateQueries({
        queryKey: ["crew-wiki-events", "scope"],
        exact: true,
      });
    });
    await waitFor(() => screen.getByTestId("wiki-read-unavailable"));
    assert.equal(screen.queryByText("Accepted Wiki article body"), null);
    assert.equal(screen.queryByText("Never generated"), null);
    assert.match(
      screen.getByTestId("wiki-read-unavailable").textContent ?? "",
      /owner scope unavailable/,
    );
  } finally {
    mounted.dispose();
  }
});

test("Project Wiki revokes accepted content when a later scope capture fails", async () => {
  const mounted = mount();
  try {
    await waitFor(() => screen.getByText("Never generated"));
    mode.scope = "error";
    await act(async () => {
      await mounted.client.invalidateQueries({
        queryKey: ["crew-wiki-events", "scope"],
        exact: true,
      });
    });
    await waitFor(() => screen.getByTestId("wiki-read-unavailable"));
    assert.match(
      screen.getByTestId("wiki-read-unavailable").textContent ?? "",
      /owner scope unavailable/,
    );
    assert.equal(screen.queryByText("Never generated"), null);
  } finally {
    mounted.dispose();
  }
});
