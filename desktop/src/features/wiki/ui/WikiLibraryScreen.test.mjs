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
    if (specifier === "@/shared/theme/ThemeProvider") {
      return { shortCircuit: true, url: "buzz-wiki-stub:theme" };
    }
    if (specifier === "@/app/navigation/useAppNavigation") {
      return { shortCircuit: true, url: "buzz-wiki-stub:useAppNavigation" };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url === "buzz-wiki-stub:theme") {
      return {
        format: "module",
        shortCircuit: true,
        source: "export const useTheme = () => ({ isDark: true });",
      };
    }
    if (url === "buzz-wiki-stub:useAppNavigation") {
      return {
        format: "module",
        shortCircuit: true,
        source:
          "export const useAppNavigation = () => ({ goProject: (id, behavior) => { globalThis.__wikiProjectVisits.push(id); globalThis.__wikiProjectVisitOptions.push(behavior ?? null); } });\n",
      };
    }
    return nextLoad(url, context);
  },
});

globalThis.__wikiProjectVisits = [];
globalThis.__wikiProjectVisitOptions = [];

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
  Node: dom.window.Node,
  NodeFilter: dom.window.NodeFilter,
  HTMLInputElement: dom.window.HTMLInputElement,
  MutationObserver: dom.window.MutationObserver,
  CustomEvent: dom.window.CustomEvent,
  getComputedStyle: dom.window.getComputedStyle,
  IS_REACT_ACT_ENVIRONMENT: true,
  localStorage: dom.window.localStorage,
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

function repository(localWorkspacePath, overrides = {}) {
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
    ...overrides,
  };
}

function repoState(branch = "release") {
  const commit = "b".repeat(40);
  return {
    id: "f".repeat(64),
    pubkey: OWNER,
    created_at: 2,
    kind: 30618,
    tags: [
      ["d", REPO_D],
      ["HEAD", `ref: refs/heads/${branch}`],
      [`refs/heads/${branch}`, commit],
    ],
    content: "",
    sig: "1".repeat(128),
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

function generationJob() {
  return {
    id: "generation-operation",
    revision: 0,
    resourceKey: `30617:${OWNER}:${REPO_D}`,
    status: "preparing",
    reconciled: false,
    snapshotId: "",
    sourceRevision: "",
    cadence: "manual",
    pages: 0,
    attempts: 0,
    progress: "generation",
    headAttempted: false,
    cancelRequested: false,
    reconcileOnly: false,
    retiredDependencyId: null,
    retryAt: 0,
    lastError: null,
  };
}

function canceledGenerationJob() {
  return {
    ...generationJob(),
    revision: 1,
    status: "canceled",
    reconciled: true,
    cancelRequested: true,
    lastError: "Wiki generation was interrupted before publication.",
  };
}

function missingSourceJob() {
  return {
    ...generationJob(),
    revision: 1,
    status: "canceled",
    reconciled: true,
    cancelRequested: true,
    lastError: "Source workspace is missing or not a directory.",
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
const { CommunitiesProvider } = await import(
  "@/features/communities/useCommunities.tsx"
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
async function mountDetail(localWorkspacePath, calls, options = {}) {
  const expected = scope();
  const originalFetchEvents = relayClient.fetchEvents;
  relayClient.fetchEvents = async () => [];
  let listedJob = options.job ?? retiredJob();
  installTauriInvoke(async (command, args) => {
    calls.push({ command, args });
    if (command === "owner_operation_scope") return expected;
    if (command === "wiki_runtime_settings_get") {
      return {
        token: expected,
        value: options.runtimeSettings ?? {
          runtimeId: "hermes",
          profile: "saved-profile",
          model: null,
        },
      };
    }
    if (command === "wiki_snapshot_read") {
      return {
        token: expected,
        value: {
          state: "missing",
          head: null,
          manifest: null,
          pages: [],
          repo_state: options.repoState ?? null,
        },
      };
    }
    if (command === "wiki_runtime_settings_set") {
      if (options.saveError) throw new Error(options.saveError);
      return { token: expected, value: args.selection };
    }
    if (command === "discover_acp_providers")
      return [
        {
          id: "hermes",
          label: "Hermes",
          availability: "available",
          default_args: [],
          source: "builtin",
        },
        {
          id: "codex",
          label: "Codex",
          availability: "available",
          default_args: [],
          source: "builtin",
        },
      ];
    if (command === "get_relay_http_url") return COMMUNITY;
    if (command === "get_media_proxy_port") return null;
    if (command === "list_hermes_profiles")
      return ["saved-profile", "other-profile"];
    if (command === "wiki_publication_prepare") {
      if (options.prepare) {
        listedJob = generationJob();
        return await options.prepare;
      }
      throw new Error("fixture stops after launch");
    }
    if (command === "wiki_publication_list") {
      return { token: expected, value: options.noJob ? [] : [listedJob] };
    }
    if (command === "wiki_publication_cancel") {
      listedJob = canceledGenerationJob();
      return { token: expected, value: listedJob };
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
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      // useWikiPublicationRecovery's mutation is created by the real screen;
      // zero GC keeps its post-unmount timeout out of this short-lived client.
      mutations: { gcTime: 0 },
    },
  });
  // useProjectsQuery has a positive staleTime, so seeding its cache prevents
  // any repository fetch.
  client.setQueryData(
    ["projects"],
    [
      {
        id: "project-1",
        repositories: [
          repository(localWorkspacePath, options.repository ?? {}),
        ],
      },
    ],
  );

  const view = render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(
        CommunitiesProvider,
        null,
        React.createElement(WikiLibraryScreen),
      ),
    ),
  );

  const card = await screen.findByTestId(`wiki-repo-card-${REPO_D}`);
  if (options.openDetail !== false) {
    await act(async () => {
      fireEvent.click(card.querySelector("button"));
    });
    await waitFor(() => {
      assert.ok(
        options.noJob
          ? screen.queryByTestId("wiki-generate-mirror")
          : screen.queryByTestId("wiki-recovery-header"),
        "the detail view must render the durable recovery row",
      );
    });
  }

  return {
    client,
    dispose() {
      view.unmount();
      client.clear();
      client.unmount();
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

test("Open project uses the containing project rather than its repository id", async () => {
  const mounted = await mountDetail(LINKED_PATH, []);
  try {
    globalThis.__wikiProjectVisits.length = 0;
    const open = buttonNamed("Open project");
    assert.ok(open);
    await act(async () => {
      fireEvent.click(open);
    });
    assert.deepEqual(globalThis.__wikiProjectVisits, ["project-1"]);
  } finally {
    mounted.dispose();
  }
});

test("a missing bound workspace offers Manage workspace for the exact repository", async () => {
  const mounted = await mountDetail(LINKED_PATH, [], {
    job: missingSourceJob(),
    repoState: repoState("release"),
    openDetail: false,
  });
  try {
    const card = screen.getByTestId(`wiki-repo-card-${REPO_D}`);
    const missing = await screen.findByTestId("wiki-missing-local");
    assert.match(missing.textContent, /Project folder is gone/);
    const manage = screen.getByTestId(`wiki-manage-workspace-${REPO_D}`);
    globalThis.__wikiProjectVisits.length = 0;
    globalThis.__wikiProjectVisitOptions.length = 0;
    await act(async () => fireEvent.click(manage));
    assert.deepEqual(globalThis.__wikiProjectVisits, ["project-1"]);
    assert.deepEqual(globalThis.__wikiProjectVisitOptions, [
      { repositoryAddress: `30617:${OWNER}:${REPO_D}` },
    ]);
    assert.equal(
      card.querySelector(`[data-testid="wiki-manage-workspace-${REPO_D}"]`),
      manage,
    );
  } finally {
    mounted.dispose();
  }
});

test("saved Wiki runtime is visible before opening settings", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls);
  try {
    await waitFor(() =>
      assert.match(
        screen.getByTestId("wiki-runtime-label").textContent,
        /Hermes \/ saved-profile/,
      ),
    );
    assert.ok(
      calls.some(({ command }) => command === "wiki_runtime_settings_get"),
    );
    assert.equal(screen.queryByTestId("wiki-runtime-settings-panel"), null);
  } finally {
    mounted.dispose();
  }
});

test("Wiki generation opens a source dialog; Escape cancels without saving or launching", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls, { noJob: true });
  try {
    await act(async () =>
      fireEvent.click(screen.getByTestId("wiki-generate-mirror")),
    );
    const dialog = await screen.findByRole("dialog", { name: "Generate Wiki" });
    assert.ok(dialog.textContent.includes(LINKED_PATH));
    await act(async () => fireEvent.keyDown(dialog, { key: "Escape" }));
    await waitFor(() => assert.equal(screen.queryByRole("dialog"), null));
    assert.equal(
      calls.some(
        ({ command }) =>
          command === "wiki_runtime_settings_set" ||
          command === "wiki_publication_prepare",
      ),
      false,
    );
  } finally {
    mounted.dispose();
  }
});

test("Wiki generation dialog reads the repository state's actual branch", async () => {
  const mounted = await mountDetail(LINKED_PATH, [], {
    noJob: true,
    repoState: repoState("release"),
    repository: { defaultBranch: "main" },
  });
  try {
    await act(async () =>
      fireEvent.click(screen.getByTestId("wiki-generate-mirror")),
    );
    const dialog = await screen.findByRole("dialog", { name: "Generate Wiki" });
    assert.match(dialog.textContent, /release/);
    assert.doesNotMatch(dialog.textContent, /\bmain\b/);
  } finally {
    mounted.dispose();
  }
});

test("Start generation persists the selected runtime before reaching native prepare", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls, { noJob: true });
  try {
    await act(async () =>
      fireEvent.click(screen.getByTestId("wiki-generate-mirror")),
    );
    const start = await screen.findByRole("button", {
      name: "Start generation",
    });
    await waitFor(() => assert.equal(start.disabled, false));
    await act(async () =>
      fireEvent.change(
        screen.getByRole("combobox", { name: "Hermes profile" }),
        { target: { value: "other-profile" } },
      ),
    );
    await act(async () => {
      mounted.client.setQueryData(
        ["acp-runtimes"],
        [
          { id: "hermes", label: "Hermes", availability: "available" },
          { id: "codex", label: "Codex", availability: "available" },
        ],
      );
    });
    assert.equal(
      screen.getByRole("combobox", { name: "Hermes profile" }).value,
      "other-profile",
      "catalog refresh must preserve the user's draft",
    );
    await act(async () => fireEvent.click(start));
    await waitFor(() =>
      assert.ok(
        calls.some(({ command }) => command === "wiki_publication_prepare"),
      ),
    );
    const save = calls.findIndex(
      ({ command }) => command === "wiki_runtime_settings_set",
    );
    const prepare = calls.findIndex(
      ({ command }) => command === "wiki_publication_prepare",
    );
    assert.ok(save >= 0 && prepare > save);
    assert.equal(calls[save].args.selection.profile, "other-profile");
    assert.equal(calls[prepare].args.repoPath, LINKED_PATH);
    assert.equal(screen.queryByRole("dialog"), null);
  } finally {
    mounted.dispose();
  }
});

test("Codex generation accepts a blank optional model and records runtime default", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls, {
    noJob: true,
    runtimeSettings: { runtimeId: "codex", profile: null, model: null },
  });
  try {
    await waitFor(() =>
      assert.match(
        screen.getByTestId("wiki-runtime-label").textContent,
        /Codex \/ runtime default/,
      ),
    );
    await act(async () =>
      fireEvent.click(screen.getByTestId("wiki-generate-mirror")),
    );
    const start = await screen.findByRole("button", {
      name: "Start generation",
    });
    const model = await screen.findByRole("textbox", {
      name: "Wiki runtime model",
    });
    await act(async () =>
      fireEvent.change(model, { target: { value: "   " } }),
    );
    await waitFor(() => assert.equal(start.disabled, false));
    await act(async () => fireEvent.click(start));
    await waitFor(() =>
      assert.ok(
        calls.some(({ command }) => command === "wiki_publication_prepare"),
      ),
    );
    const save = calls.find(
      ({ command }) => command === "wiki_runtime_settings_set",
    );
    assert.deepEqual(save.args.selection, {
      runtimeId: "codex",
      model: null,
      profile: null,
    });
  } finally {
    mounted.dispose();
  }
});

test("a failed runtime save keeps the dialog open and never launches generation", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls, {
    noJob: true,
    saveError: "settings disk unavailable",
  });
  try {
    await act(async () =>
      fireEvent.click(screen.getByTestId("wiki-generate-mirror")),
    );
    const start = await screen.findByRole("button", {
      name: "Start generation",
    });
    await waitFor(() => assert.equal(start.disabled, false));
    await act(async () => fireEvent.click(start));
    await screen.findByText("settings disk unavailable");
    assert.ok(screen.queryByRole("dialog"));
    assert.equal(
      calls.some(({ command }) => command === "wiki_publication_prepare"),
      false,
    );
  } finally {
    mounted.dispose();
  }
});

test("initial native generation shows clear progress and only the real Cancel action", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls, {
    job: generationJob(),
    openDetail: false,
  });
  try {
    await waitFor(() => {
      assert.match(
        screen.getByTestId("wiki-generating").textContent,
        /Generating Wiki…/,
      );
    });
    assert.equal(screen.queryByText("0/0"), null);
    assert.ok(screen.getByRole("button", { name: "Cancel job" }));
    assert.doesNotMatch(
      screen.getByTestId("wiki-recovery-controls").textContent,
      /attempts/,
    );
    assert.equal(
      screen.queryByRole("button", { name: "Retry publication" }),
      null,
    );
    assert.equal(
      screen.queryByRole("button", { name: "Resume publication" }),
      null,
    );
    assert.equal(screen.queryByRole("button", { name: "Reconcile" }), null);

    await act(async () => {
      fireEvent.click(
        screen.getByTestId(`wiki-repo-card-${REPO_D}`).querySelector("button"),
      );
    });
    await waitFor(() => screen.getByTestId("wiki-recovery-header"));
    assert.match(
      screen.getByTestId("wiki-recovery-header").textContent,
      /Generating Wiki…/,
    );
    assert.ok(screen.getByRole("button", { name: "Cancel" }));
    assert.equal(screen.queryByRole("button", { name: "Reconcile" }), null);
    assert.equal(
      screen.queryByRole("button", { name: "Retry publication" }),
      null,
    );

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    });
    await waitFor(() => {
      assert.ok(
        calls.some(({ command }) => command === "wiki_publication_cancel"),
      );
      assert.ok(screen.getByTestId("wiki-generation-canceled"));
    });
    const cancel = calls.find(
      ({ command }) => command === "wiki_publication_cancel",
    );
    assert.equal(cancel.args.id, generationJob().id);
    assert.equal(cancel.args.revision, generationJob().revision);
    assert.match(
      screen.getByTestId("wiki-generation-canceled").textContent,
      /interrupted before publication/,
    );
    assert.match(
      screen.getByTestId("wiki-recovery-header").textContent,
      /Generation: canceled/,
    );
    assert.doesNotMatch(
      screen.getByTestId("wiki-recovery-header").textContent,
      /Generating Wiki…/,
    );
    assert.equal(
      screen.getByTestId("wiki-generate-mirror").disabled,
      false,
      "a terminal canceled draft permits a new Generate",
    );
  } finally {
    mounted.dispose();
  }
});

test("a canceled generation does not duplicate its durable prepare error", async () => {
  const calls = [];
  const message = canceledGenerationJob().lastError;
  let rejectPrepare;
  const prepare = new Promise((_, reject) => {
    rejectPrepare = reject;
  });
  const mounted = await mountDetail(LINKED_PATH, calls, {
    job: { ...canceledGenerationJob(), id: "previous-generation" },
    prepare,
  });
  try {
    await act(async () =>
      fireEvent.click(screen.getByTestId("wiki-generate-mirror")),
    );
    const start = await screen.findByRole("button", {
      name: "Start generation",
    });
    await waitFor(() => assert.equal(start.disabled, false));
    await act(async () => fireEvent.click(start));
    await waitFor(() =>
      assert.ok(
        calls.some(({ command }) => command === "wiki_publication_prepare"),
      ),
    );
    const cancel = await screen.findByRole(
      "button",
      { name: "Cancel", exact: true },
      { timeout: 5000 },
    );
    await act(async () => fireEvent.click(cancel));
    await waitFor(() =>
      assert.ok(screen.queryByTestId("wiki-generation-canceled")),
    );
    await act(async () => rejectPrepare(new Error(message)));
    await waitFor(() =>
      assert.ok(
        mounted.client
          .getMutationCache()
          .getAll()
          .some((candidate) => candidate.state.status === "error"),
      ),
    );
    assert.equal(screen.getAllByText(message).length, 1);
    assert.equal(screen.queryByTestId("wiki-generate-error") === null, true);
    assert.equal(
      screen.getByTestId("wiki-generation-canceled").textContent,
      message,
    );
  } finally {
    mounted.dispose();
  }
});

test("a terminal canceled generation stays visible and permits Generate", async () => {
  const calls = [];
  const mounted = await mountDetail(LINKED_PATH, calls, {
    job: canceledGenerationJob(),
    openDetail: false,
  });
  try {
    await waitFor(() => screen.getByTestId("wiki-generation-canceled"));
    assert.equal(screen.queryByRole("button", { name: "Cancel job" }), null);
    assert.equal(screen.queryByRole("button", { name: "Reconcile" }), null);
    assert.equal(screen.getByTestId("wiki-generate-crew").disabled, false);
  } finally {
    mounted.dispose();
  }
});
