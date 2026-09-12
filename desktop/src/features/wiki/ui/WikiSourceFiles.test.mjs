import assert from "node:assert/strict";
import { after, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

const OWNER = "a".repeat(64);
const REPO_D = "crew";
const COORDINATE = `30617:${OWNER}:${REPO_D}`;
const SOURCE_PATH = "src/runtime.ts";
const scope = {
  scope: { owner: OWNER, community: "https://relay.example" },
  workspace_generation: 1,
  identity_generation: 1,
};
const foreignScope = {
  ...scope,
  scope: { ...scope.scope, community: "https://other.example" },
};
const pageEvent = {
  id: "f".repeat(64),
  pubkey: OWNER,
  kind: 30623,
  content: "Wiki page",
  created_at: 10,
  sig: "",
  tags: [
    [
      "wiki-source-files",
      JSON.stringify([[SOURCE_PATH, "d".repeat(64), 2, 2, 3]]),
    ],
  ],
};

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});

const { act, cleanup, fireEvent, render, screen, waitFor } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { WikiSourceFiles } = await import("./WikiSourceFiles.tsx");

function grant(token = scope) {
  return {
    capabilityId: "capability-1",
    repositoryCoordinate: COORDINATE,
    token,
    label: "Crew checkout",
    workspaceMode: "git",
  };
}

function installBridge(
  calls,
  {
    granted = false,
    failOpen = false,
    failGrants = false,
    deferOpen = false,
    grantToken = scope,
  } = {},
) {
  let resolveOpen;
  const bridge = {
    transformCallback: () => 1,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "wiki_source_grants") {
        if (failGrants) throw new Error("grant lookup unavailable");
        return granted ? [grant(grantToken)] : [];
      }
      if (command === "wiki_open_verified_source") {
        if (failOpen) throw new Error("source unavailable");
        if (deferOpen) {
          return new Promise((resolve) => {
            resolveOpen = resolve;
          });
        }
        return { content: "line 1\nline 2\nline 3", startLine: 2, endLine: 3 };
      }
      if (command === "wiki_forget_source_root") return null;
      throw new Error(`Unexpected command ${command}`);
    },
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  dom.window.__TAURI_INTERNALS__ = bridge;
  return {
    resolveOpen: (
      value = { content: "line 1\nline 2", startLine: 1, endLine: 2 },
    ) => resolveOpen?.(value),
  };
}

function mount(client, event = pageEvent, operationScope = scope) {
  return render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(WikiSourceFiles, {
        files: [SOURCE_PATH],
        owner: OWNER,
        operationScope,
        pageEvent: event,
        repoD: REPO_D,
      }),
    ),
  );
}

function queryClient() {
  return new QueryClient({
    // Mutations retain a five-minute GC timer by default; zero keeps each
    // source test's client disposable so the natural Node test exit is real.
    defaultOptions: {
      mutations: { gcTime: 0 },
      queries: { retry: false, gcTime: 0 },
    },
  });
}

beforeEach(() => {
  cleanup();
  delete globalThis.__TAURI_INTERNALS__;
  delete dom.window.__TAURI_INTERNALS__;
});

after(() => {
  cleanup();
  dom.window.close();
});

test("source files fail closed without a grant instead of opening a current checkout link", async () => {
  const calls = [];
  installBridge(calls);
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByTestId("wiki-source-unavailable"));
    const file = screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`);
    assert.equal(file.disabled, true);
    assert.equal(
      calls.some((call) => call.command === "wiki_open_verified_source"),
      false,
    );
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("grant lookup failure stays visible as an error instead of pretending no folder is selected", async () => {
  const calls = [];
  installBridge(calls, { failGrants: true });
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByTestId("wiki-source-grants-error"));
    assert.equal(screen.queryByText("Choose folder to open source"), null);
    assert.equal(
      screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`).disabled,
      true,
    );
    await act(async () => {
      fireEvent.click(screen.getByTestId("wiki-source-grants-retry"));
    });
    assert.ok(
      calls.filter((call) => call.command === "wiki_source_grants").length >= 2,
    );
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("a granted source file opens the exact page reference through native IPC", async () => {
  const calls = [];
  installBridge(calls, { granted: true });
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByText("Folder: Crew checkout"));
    await act(async () => {
      fireEvent.click(screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`));
    });
    await waitFor(() => screen.getByTestId("wiki-source-preview"));
    assert.equal(
      screen.getByTestId("wiki-source-preview").textContent,
      "line 2\nline 3",
    );
    const openCall = calls.find(
      (call) => call.command === "wiki_open_verified_source",
    );
    assert.deepEqual(openCall.args, {
      capabilityId: "capability-1",
      page: pageEvent,
      referenceIndex: 0,
    });
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("pane mode opens the requested immutable citation and exposes a dismiss action", async () => {
  const calls = [];
  installBridge(calls, { granted: true });
  const client = queryClient();
  let closed = false;
  const view = render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(WikiSourceFiles, {
        files: [SOURCE_PATH],
        initialRequest: {
          path: SOURCE_PATH,
          startLine: 2,
          endLine: 3,
        },
        mode: "pane",
        onClosePane: () => {
          closed = true;
        },
        operationScope: scope,
        owner: OWNER,
        pageEvent,
        repoD: REPO_D,
      }),
    ),
  );
  try {
    await waitFor(() => screen.getByTestId("wiki-source-pane"));
    await waitFor(() => screen.getByTestId("wiki-source-preview"));
    assert.equal(
      screen.getByTestId("wiki-source-preview").textContent,
      "line 2\nline 3",
    );
    fireEvent.click(screen.getByTestId("wiki-source-pane-close"));
    assert.equal(closed, true);
    assert.equal(
      calls.filter((call) => call.command === "wiki_open_verified_source")
        .length,
      1,
    );
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("native source failure remains visible as unavailable instead of falling back", async () => {
  const calls = [];
  installBridge(calls, { granted: true, failOpen: true });
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByText("Folder: Crew checkout"));
    await act(async () => {
      fireEvent.click(screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`));
    });
    await waitFor(() => screen.getByText("source unavailable"));
    assert.equal(screen.queryByTestId("wiki-source-preview"), null);
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("source grants from another community stay unavailable", async () => {
  const calls = [];
  installBridge(calls, { granted: true, grantToken: foreignScope });
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByTestId("wiki-source-unavailable"));
    assert.equal(
      screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`).disabled,
      true,
    );
    assert.equal(
      calls.some((call) => call.command === "wiki_open_verified_source"),
      false,
    );
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("a delayed source result is discarded when the cited page changes", async () => {
  const calls = [];
  const deferred = installBridge(calls, { granted: true, deferOpen: true });
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByText("Folder: Crew checkout"));
    await act(async () => {
      fireEvent.click(screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`));
    });
    const nextPage = {
      ...pageEvent,
      id: "e".repeat(64),
    };
    view.rerender(
      React.createElement(
        QueryClientProvider,
        { client },
        React.createElement(WikiSourceFiles, {
          files: [SOURCE_PATH],
          operationScope: scope,
          owner: OWNER,
          pageEvent: nextPage,
          repoD: REPO_D,
        }),
      ),
    );
    await act(async () => {
      deferred.resolveOpen();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    assert.equal(screen.queryByTestId("wiki-source-preview"), null);
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("a delayed source result is discarded when the owner scope changes", async () => {
  const calls = [];
  const pending = installBridge(calls, { granted: true, deferOpen: true });
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByText("Folder: Crew checkout"));
    await act(async () => {
      fireEvent.click(screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`));
    });
    view.rerender(
      React.createElement(
        QueryClientProvider,
        { client },
        React.createElement(WikiSourceFiles, {
          files: [SOURCE_PATH],
          operationScope: foreignScope,
          owner: OWNER,
          pageEvent,
          repoD: REPO_D,
        }),
      ),
    );
    await act(async () => {
      pending.resolveOpen();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    assert.equal(screen.queryByTestId("wiki-source-preview"), null);
    assert.ok(
      calls.filter((call) => call.command === "wiki_source_grants").length >= 2,
    );
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});

test("forgetting a source folder fences an in-flight source result", async () => {
  const calls = [];
  const deferred = installBridge(calls, { granted: true, deferOpen: true });
  const client = queryClient();
  const view = mount(client);
  try {
    await waitFor(() => screen.getByText("Folder: Crew checkout"));
    await act(async () => {
      fireEvent.click(screen.getByTestId(`wiki-source-file-${SOURCE_PATH}`));
      fireEvent.click(screen.getByText("Forget folder"));
    });
    await waitFor(() =>
      assert.ok(
        calls.some((call) => call.command === "wiki_forget_source_root"),
      ),
    );
    await act(async () => {
      deferred.resolveOpen();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    assert.equal(screen.queryByTestId("wiki-source-preview"), null);
  } finally {
    await act(async () => view.unmount());
    client.clear();
    client.unmount();
  }
});
