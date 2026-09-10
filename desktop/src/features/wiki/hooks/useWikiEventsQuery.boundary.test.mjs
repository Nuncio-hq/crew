import assert from "node:assert/strict";
import { after, test } from "node:test";
import { JSDOM } from "jsdom";

const OWNER = "a".repeat(64);
const COMMUNITY = "https://relay.example";
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});

function scope(identityGeneration = 1) {
  return {
    scope: { owner: OWNER, community: COMMUNITY },
    workspace_generation: 1,
    identity_generation: identityGeneration,
  };
}

function coordinate(repoD) {
  return `30617:${OWNER}:${repoD}`;
}

function repository(repoD) {
  return {
    id: repoD,
    dtag: repoD,
    name: repoD,
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
    repoAddress: coordinate(repoD),
    channelId: null,
  };
}

function snapshot(state = "missing", repoState = null) {
  return {
    state,
    head: null,
    manifest: null,
    pages: [],
    repoState,
  };
}

function installTauriInvoke(handler) {
  const bridge = {
    invoke: handler,
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  dom.window.__TAURI_INTERNALS__ = bridge;
}

const { readWikiSnapshotAtScope, WikiSnapshotScopeChanged } = await import(
  "@/shared/api/wikiSnapshot.ts"
);
const { estimateWikiSnapshotBytes, wikiRepositoryCoordinate } = await import(
  "@/shared/api/wikiSnapshot.ts"
);
const { projectWikiRepositoryReads, planWikiRepositoryReads } = await import(
  "./wikiReadProjection.ts"
);

after(() => dom.window.close());

test("a scope change after the native Wiki IPC result is rejected", async () => {
  const expected = scope(1);
  installTauriInvoke(async (command) => {
    if (command === "wiki_snapshot_read") {
      return {
        token: expected,
        value: snapshot(),
      };
    }
    if (command === "owner_operation_scope") return scope(2);
    throw new Error(`Unexpected Tauri command: ${command}`);
  });

  await assert.rejects(
    readWikiSnapshotAtScope(coordinate("crew"), expected),
    (error) => error instanceof WikiSnapshotScopeChanged,
  );
});

test("automatic Wiki reads stop at the fixed bound and preserve priority order", () => {
  const coordinates = Array.from({ length: 130 }, (_, index) =>
    coordinate(`repo-${index}`),
  );
  const priority = [coordinates[129], coordinates[128]];
  const plan = planWikiRepositoryReads(coordinates, priority);
  assert.deepEqual(plan.prioritized, priority);
  assert.equal(plan.automatic.length, 128);
  assert.equal(plan.skipped.length, 0);
  assert.equal(new Set([...plan.prioritized, ...plan.automatic]).size, 130);
});

test("an incomplete refresh keeps the previous coherent graph as stale", () => {
  const repoState = {
    id: "state-1",
    kind: 30618,
    pubkey: OWNER,
    content: "",
    created_at: 10,
    tags: [["d", "crew"]],
    sig: "",
  };
  const previousSnapshot = {
    state: "complete",
    head: {
      id: "toc-1",
      kind: 30623,
      pubkey: OWNER,
      content: "{}",
      created_at: 9,
      tags: [
        ["d", "crew/_toc"],
        ["a", `${30617}:${OWNER}:crew`],
      ],
      sig: "",
    },
    manifest: null,
    pages: [],
    repoState,
  };
  const key = coordinate("crew");
  const previous = new Map([
    [
      key,
      {
        scope: scope(),
        coordinate: key,
        snapshot: previousSnapshot,
        repoState,
        repoStateFresh: true,
        serializedBytes: estimateWikiSnapshotBytes(previousSnapshot),
        status: {
          coordinate: key,
          state: "ready",
          outcome: "complete",
          stale: false,
          unavailable: false,
          message: null,
        },
      },
    ],
  ]);
  const projected = projectWikiRepositoryReads(
    scope(),
    [key],
    [
      {
        coordinate: key,
        error: "native snapshot incomplete",
        outcome: "incomplete",
      },
    ],
    [],
    previous,
  );
  const value = projected.get(key);
  assert.equal(value.snapshot?.head?.id, "toc-1");
  assert.equal(value.repoStateFresh, false);
  assert.equal(value.repoState?.id, "state-1");
  assert.equal(value.status.state, "stale");
  assert.equal(value.status.unavailable, true);
});

/**
 * A priority-only correction pass emits `{error:"", outcome:"preserved"}` for
 * every coordinate it did not re-read. Those entries must be kept exactly as
 * they were — including entries whose snapshot is null, whose accurate prior
 * status is `missing`, `incomplete` or an explicit error. Overwriting them
 * with a generic unavailable row loses real repository state the coordinator
 * never contradicted.
 */
test("a preserved correction pass keeps non-renderable prior entries verbatim", () => {
  const priorFor = (name, status, message, repoState = null) => {
    const key = coordinate(name);
    return [
      key,
      {
        scope: scope(),
        coordinate: key,
        snapshot: null,
        repoState,
        repoStateFresh: repoState !== null,
        serializedBytes: 0,
        status: {
          coordinate: key,
          state: "unavailable",
          outcome: status,
          stale: false,
          unavailable: true,
          message,
        },
      },
    ];
  };
  const missingState = {
    id: "state-missing",
    kind: 30618,
    pubkey: OWNER,
    content: "",
    created_at: 4,
    tags: [["d", "missing"]],
    sig: "",
  };
  const entries = [
    priorFor("missing", "missing", null, missingState),
    priorFor("incomplete", "incomplete", "native snapshot incomplete"),
    priorFor("errored", "error", "relay unreachable"),
  ];
  const previous = new Map(entries);
  const keys = entries.map(([key]) => key);

  const projected = projectWikiRepositoryReads(
    scope(),
    keys,
    keys.map((key) => ({ coordinate: key, error: "", outcome: "preserved" })),
    [],
    previous,
  );

  for (const [key, prior] of entries) {
    const value = projected.get(key);
    assert.deepEqual(
      value,
      prior,
      `the whole prior entry for ${key} must be preserved verbatim`,
    );
    assert.equal(value.status.outcome, prior.status.outcome);
    assert.equal(value.status.message, prior.status.message);
    assert.equal(value.repoState, prior.repoState);
  }
});

test("preservation still rejects a foreign scope and an over-budget graph", () => {
  const key = coordinate("crew");
  const foreignScope = { ...scope(), identity_generation: 99 };
  const nullEntry = {
    scope: foreignScope,
    coordinate: key,
    snapshot: null,
    repoState: null,
    repoStateFresh: false,
    serializedBytes: 0,
    status: {
      coordinate: key,
      state: "unavailable",
      outcome: "incomplete",
      stale: false,
      unavailable: true,
      message: "native snapshot incomplete",
    },
  };
  const foreign = projectWikiRepositoryReads(
    scope(),
    [key],
    [{ coordinate: key, error: "", outcome: "preserved" }],
    [],
    new Map([[key, nullEntry]]),
  ).get(key);
  assert.notDeepEqual(
    foreign,
    nullEntry,
    "an A -> B -> A scope must not inherit another generation's entry",
  );
  assert.equal(foreign.status.outcome, "error");

  const bigSnapshot = {
    state: "complete",
    head: {
      id: "toc-big",
      kind: 30623,
      pubkey: OWNER,
      content: "{}",
      created_at: 9,
      tags: [
        ["d", "crew/_toc"],
        ["a", `${30617}:${OWNER}:crew`],
      ],
      sig: "",
    },
    manifest: null,
    pages: [],
    repoState: null,
  };
  const overBudget = projectWikiRepositoryReads(
    scope(),
    [key],
    [{ coordinate: key, error: "", outcome: "preserved" }],
    [],
    new Map([
      [
        key,
        {
          scope: scope(),
          coordinate: key,
          snapshot: bigSnapshot,
          repoState: null,
          repoStateFresh: false,
          serializedBytes: estimateWikiSnapshotBytes(bigSnapshot),
          status: {
            coordinate: key,
            state: "ready",
            outcome: "complete",
            stale: false,
            unavailable: false,
            message: null,
          },
        },
      ],
    ]),
    0,
  ).get(key);
  assert.equal(
    overBudget.snapshot,
    null,
    "a graph that cannot fit the retained budget is not preserved",
  );
});

test("company Wiki remains available while a repository read is stalled", async () => {
  const { act, cleanup, renderHook, waitFor } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  const expected = scope();
  let releaseRead;
  const readGate = new Promise((resolve) => {
    releaseRead = resolve;
  });
  const companyPage = {
    id: "company-1",
    kind: 30023,
    pubkey: OWNER,
    content: "Company guidance",
    created_at: 20,
    tags: [
      ["d", "handbook"],
      ["title", "Handbook"],
    ],
    sig: "",
  };
  const originalFetchEvents = relayClient.fetchEvents;
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return expected;
    if (command === "wiki_snapshot_read") {
      await readGate;
      return { token: expected, value: snapshot() };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  });
  relayClient.fetchEvents = async (filter) => {
    assert.deepEqual(filter, { kinds: [30023], limit: 200 });
    return [companyPage];
  };
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const repo = repository("crew");
  const hook = renderHook(() => useWikiEventsQuery([repo]), {
    wrapper: ({ children }) =>
      React.createElement(QueryClientProvider, { client }, children),
  });
  try {
    await waitFor(() => {
      assert.equal(hook.result.current.data?.company[0]?.slug, "handbook");
    });
    assert.equal(hook.result.current.data?.companyError, null);
    assert.equal(hook.result.current.isFetching, true);
  } finally {
    await act(async () => releaseRead());
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    relayClient.fetchEvents = originalFetchEvents;
  }
});

test("a repository selected during an active batch receives a priority retry", async () => {
  const { act, renderHook, waitFor, cleanup } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  const expected = scope();
  const repositories = [
    repository("one"),
    repository("two"),
    repository("three"),
  ];
  const coordinates = repositories.map((repo) =>
    wikiRepositoryCoordinate(repo.owner, repo.dtag),
  );
  const calls = [];
  let releaseRead;
  const readGate = new Promise((resolve) => {
    releaseRead = resolve;
  });
  let released = false;
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return expected;
    if (command !== "wiki_snapshot_read")
      throw new Error(`Unexpected Tauri command: ${command}`);
    calls.push(args.coordinate);
    if (!released) await readGate;
    return { token: expected, value: snapshot("missing") };
  });
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  client.setQueryData(["projects"], [{ repositories }]);
  const hook = renderHook(
    ({ priorities }) =>
      useWikiEventsQuery(undefined, { priorityCoordinates: priorities }),
    {
      initialProps: { priorities: [] },
      wrapper: ({ children }) =>
        React.createElement(QueryClientProvider, { client }, children),
    },
  );
  try {
    await waitFor(() => assert.ok(calls.length >= 2));
    await act(async () => {
      hook.rerender({ priorities: [coordinates[2]] });
    });
    released = true;
    await act(async () => releaseRead());
    await waitFor(() => assert.equal(hook.result.current.isFetching, false));
    assert.ok(
      calls.slice(3).includes(coordinates[2]),
      `expected a post-batch priority read for ${coordinates[2]}, got ${calls.join(", ")}`,
    );
  } finally {
    released = true;
    releaseRead();
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
  }
});

/**
 * Item 7: the real coordinator path. A second consumer mounts and changes the
 * priority set while the first read is still in flight, so the coordinator
 * runs its priority-only correction pass and emits `preserved` for the
 * repository it did not re-read. That repository's accurate prior status must
 * survive the pass instead of collapsing into a generic unavailable row.
 */
test("a mounted priority correction preserves a non-priority repository's prior status", async () => {
  const { renderHook, waitFor, cleanup } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  const expected = scope();
  const stalled = repository("stalled");
  const selected = repository("selected");
  const stalledKey = coordinate("stalled");
  const selectedKey = coordinate("selected");
  const originalFetchEvents = relayClient.fetchEvents;
  const reads = [];
  let releaseFirstPass;
  const firstPassGate = new Promise((resolve) => {
    releaseFirstPass = resolve;
  });
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return expected;
    if (command === "wiki_snapshot_read") {
      reads.push(args.coordinate);
      // Hold the first pass open so the priority revision below lands while
      // it is still in flight, which is what makes the coordinator run its
      // correction pass rather than a fresh full read.
      if (reads.length <= 2) await firstPassGate;
      if (args.coordinate === stalledKey) {
        // A real incomplete read: accurate, specific, and not renderable.
        return {
          token: expected,
          value: {
            state: "incomplete",
            head: null,
            manifest: null,
            pages: [],
            error: "native snapshot incomplete",
          },
        };
      }
      return { token: expected, value: snapshot("complete") };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  });
  relayClient.fetchEvents = async () => [];
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const wrapper = ({ children }) =>
    React.createElement(QueryClientProvider, { client }, children);
  // Seed the canonical project collection so the first consumer contributes
  // both coordinates without making either one a priority. The second
  // consumer below then adds only `selectedKey` to the priority set, which is
  // the correction-pass shape this test is meant to exercise.
  client.setQueryData(["projects"], [{ repositories: [stalled, selected] }]);
  const firstHook = renderHook(() => useWikiEventsQuery(), { wrapper });
  try {
    // Both coordinates enter the first pass, which is still held.
    await waitFor(() => {
      assert.equal(reads.length, 2, JSON.stringify(reads));
    });

    // A second consumer selects the other repository, revising the priority
    // order while the first pass is in flight. The production signature takes
    // `WikiEventsQueryOptions`, so the priority must be an option field: an
    // array here is silently ignored and no correction is ever requested.
    const secondHook = renderHook(
      () =>
        useWikiEventsQuery([selected], { priorityCoordinates: [selectedKey] }),
      { wrapper },
    );
    releaseFirstPass();
    try {
      await waitFor(() => {
        assert.equal(
          firstHook.result.current.data?.repositoryStatuses[stalledKey]
            ?.outcome,
          "incomplete",
        );
      });
      const before =
        firstHook.result.current.data.repositoryStatuses[stalledKey];
      assert.equal(before.message, "native snapshot incomplete");

      // The correction pass must not merely *start*: wait until its request
      // has landed AND the resulting projection is rendered, otherwise the
      // comparison below could still be reading the first projection.
      await waitFor(() => {
        assert.ok(
          reads.slice(2).includes(selectedKey),
          `the revised priority must be re-read: ${JSON.stringify(reads)}`,
        );
      });
      await waitFor(() => {
        assert.equal(
          reads.filter((key) => key === selectedKey).length,
          2,
          `the selected coordinate is read exactly twice: ${JSON.stringify(reads)}`,
        );
        assert.equal(
          secondHook.result.current.isFetching,
          false,
          "the correction pass must have completed",
        );
        assert.equal(
          secondHook.result.current.data?.repositoryStatuses[selectedKey]
            ?.state,
          "ready",
          "the corrected projection is rendered",
        );
      });

      const after =
        firstHook.result.current.data.repositoryStatuses[stalledKey];
      // Falsifiable in two independent ways: the non-priority coordinate must
      // never be re-read, and its accurate prior status object must survive
      // the correction pass verbatim. Restoring the old `renderable(previous)`
      // guard turns this entry into a generic unavailable row and fails here.
      assert.equal(
        reads.filter((key) => key === stalledKey).length,
        1,
        `the non-priority coordinate must not be refetched: ${JSON.stringify(reads)}`,
      );
      assert.deepEqual(
        after,
        before,
        "the whole prior status object must be preserved",
      );
      assert.equal(after.outcome, "incomplete");
      assert.equal(after.message, "native snapshot incomplete");
      assert.notEqual(
        after.message,
        "Wiki read failed.",
        "a preserved entry must not collapse into the generic failure",
      );
    } finally {
      secondHook.unmount();
    }
  } finally {
    releaseFirstPass();
    firstHook.unmount();
    relayClient.fetchEvents = originalFetchEvents;
    client.clear();
    cleanup();
  }
});

test("two mounted Wiki consumers share one canonical scoped query and the global read bound", async () => {
  const { renderHook, waitFor, cleanup } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  const expected = scope();
  const first = repository("first");
  const second = repository("second");
  const calls = [];
  let active = 0;
  let peak = 0;
  const originalFetchEvents = relayClient.fetchEvents;
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return expected;
    if (command === "wiki_snapshot_read") {
      calls.push(args.coordinate);
      active += 1;
      peak = Math.max(peak, active);
      await Promise.resolve();
      active -= 1;
      return { token: expected, value: snapshot("complete") };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  });
  relayClient.fetchEvents = async () => [];
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const wrapper = ({ children }) =>
    React.createElement(QueryClientProvider, { client }, children);
  const firstHook = renderHook(() => useWikiEventsQuery([first]), { wrapper });
  const secondHook = renderHook(() => useWikiEventsQuery([second]), {
    wrapper,
  });
  try {
    await waitFor(() => {
      assert.equal(
        firstHook.result.current.data?.repositoryStatuses[coordinate("first")]
          ?.state,
        "ready",
      );
      assert.equal(
        secondHook.result.current.data?.repositoryStatuses[coordinate("second")]
          ?.state,
        "ready",
      );
    });
    const repoQueries = client
      .getQueryCache()
      .findAll({ queryKey: ["crew-wiki-events", "repos"] });
    assert.equal(
      repoQueries.filter((query) => query.queryKey[2] === OWNER).length,
      1,
      JSON.stringify(repoQueries.map((query) => query.queryKey)),
    );
    assert.ok(calls.includes(coordinate("first")));
    assert.ok(calls.includes(coordinate("second")));
    assert.ok(peak <= 2, `global Wiki read peak was ${peak}`);
  } finally {
    firstHook.unmount();
    secondHook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    relayClient.fetchEvents = originalFetchEvents;
  }
});

test("two mounted Wiki consumers adopt the same validated newer scope", async () => {
  const { act, renderHook, waitFor, cleanup } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { resetWikiStore } = await import("@/features/wiki/lib/wikiStore");
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  resetWikiStore();
  let activeScope = scope(21);
  const first = repository("scope-first");
  const second = repository("scope-second");
  const originalFetchEvents = relayClient.fetchEvents;
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return activeScope;
    if (command === "wiki_snapshot_read") {
      return { token: activeScope, value: snapshot("missing") };
    }
    throw new Error(`Unexpected command: ${command}`);
  });
  relayClient.fetchEvents = async () => [];
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const wrapper = ({ children }) =>
    React.createElement(QueryClientProvider, { client }, children);
  const firstHook = renderHook(() => useWikiEventsQuery([first]), { wrapper });
  const secondHook = renderHook(() => useWikiEventsQuery([second]), {
    wrapper,
  });
  try {
    await waitFor(() => {
      assert.equal(
        firstHook.result.current.data?.repositoryStatuses[
          coordinate("scope-first")
        ]?.coordinate,
        coordinate("scope-first"),
      );
      assert.equal(
        secondHook.result.current.data?.repositoryStatuses[
          coordinate("scope-second")
        ]?.coordinate,
        coordinate("scope-second"),
      );
    });
    activeScope = scope(22);
    await act(async () => {
      await firstHook.result.current.scopeQuery.refetch();
    });
    await waitFor(() => {
      assert.equal(
        firstHook.result.current.scopeQuery.data?.identity_generation,
        22,
      );
      assert.equal(
        secondHook.result.current.scopeQuery.data?.identity_generation,
        22,
      );
      assert.equal(
        firstHook.result.current.data?.repositoryStatuses[
          coordinate("scope-first")
        ]?.coordinate,
        coordinate("scope-first"),
      );
      assert.equal(
        secondHook.result.current.data?.repositoryStatuses[
          coordinate("scope-second")
        ]?.coordinate,
        coordinate("scope-second"),
      );
    });
  } finally {
    firstHook.unmount();
    secondHook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    relayClient.fetchEvents = originalFetchEvents;
    resetWikiStore();
  }
});

test("last Wiki consumer retires the mounted QueryClient graph immediately", async () => {
  const { renderHook, waitFor, cleanup } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  const expected = scope();
  const repo = repository("retained-graph");
  const originalFetchEvents = relayClient.fetchEvents;
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return expected;
    if (command === "wiki_snapshot_read") {
      return { token: expected, value: snapshot("complete") };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  });
  relayClient.fetchEvents = async () => [];
  // Deliberately keep the client default long-lived. The hook's scoped query
  // policy must retire the graph independently of unrelated QueryClient
  // defaults.
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 5 * 60_000 } },
  });
  const hook = renderHook(() => useWikiEventsQuery([repo]), {
    wrapper: ({ children }) =>
      React.createElement(QueryClientProvider, { client }, children),
  });
  try {
    await waitFor(() => {
      assert.equal(
        hook.result.current.data?.repositoryStatuses[
          coordinate("retained-graph")
        ]?.state,
        "ready",
      );
    });
    assert.equal(
      client
        .getQueryCache()
        .findAll({ queryKey: ["crew-wiki-events", "repos"] }).length,
      1,
    );
    hook.unmount();
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(
      client
        .getQueryCache()
        .findAll({ queryKey: ["crew-wiki-events", "repos"] }).length,
      0,
    );
    assert.equal(
      client
        .getQueryCache()
        .findAll({ queryKey: ["crew-wiki-events", "company"] }).length,
      0,
    );
  } finally {
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    relayClient.fetchEvents = originalFetchEvents;
  }
});

test("a selected repository is recovered first when the retained graph budget is full", () => {
  const selected = coordinate("selected");
  const other = coordinate("other");
  const largeSnapshot = (label) => ({
    ...snapshot("complete"),
    head: {
      id: `${label}-head`,
      kind: 30623,
      pubkey: OWNER,
      content: "x".repeat(2_000),
      created_at: 1,
      tags: [
        ["d", `${label}/_toc`],
        ["a", coordinate(label)],
      ],
      sig: "",
    },
  });
  const selectedSnapshot = largeSnapshot("selected");
  const otherSnapshot = largeSnapshot("other");
  const selectedBytes = estimateWikiSnapshotBytes(selectedSnapshot);
  const full = projectWikiRepositoryReads(
    scope(),
    [selected, other],
    [
      { coordinate: selected, snapshot: selectedSnapshot },
      { coordinate: other, snapshot: otherSnapshot },
    ],
    [],
    new Map(),
    selectedBytes * 3,
  );
  assert.ok(full.get(selected)?.snapshot);
  assert.ok(full.get(other)?.snapshot);

  const refreshedSelected = largeSnapshot("selected-refreshed");
  const recovered = projectWikiRepositoryReads(
    scope(),
    [selected, other],
    [{ coordinate: selected, snapshot: refreshedSelected }],
    [],
    full,
    estimateWikiSnapshotBytes(refreshedSelected) + 1,
    [selected],
  );
  assert.equal(
    recovered.get(selected)?.snapshot?.head?.content,
    "x".repeat(2_000),
  );
  assert.equal(recovered.get(selected)?.status.outcome, "complete");
  assert.equal(recovered.get(other)?.snapshot, null);
  assert.equal(recovered.get(other)?.status.state, "unavailable");
});

test("v1 Wiki freshness uses the immutable manifest generation timestamp", async () => {
  const { renderHook, waitFor, cleanup } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  const expected = scope();
  const repo = repository("generated");
  const head = {
    id: "head-1",
    kind: 30623,
    pubkey: OWNER,
    content: JSON.stringify({ sections: [] }),
    created_at: 20,
    tags: [
      ["d", "generated/_toc"],
      ["a", coordinate("generated")],
      ["wiki-version", "1"],
    ],
    sig: "",
  };
  const manifest = {
    id: "manifest-1",
    kind: 30623,
    pubkey: OWNER,
    content: "{}",
    created_at: 10,
    tags: [["d", "generated/m1-hash"]],
    sig: "",
  };
  const originalFetchEvents = relayClient.fetchEvents;
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return expected;
    if (command === "wiki_snapshot_read") {
      return {
        token: expected,
        value: {
          state: "complete",
          head,
          manifest,
          pages: [],
          repo_state: null,
        },
      };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  });
  relayClient.fetchEvents = async () => [];
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const hook = renderHook(() => useWikiEventsQuery([repo]), {
    wrapper: ({ children }) =>
      React.createElement(QueryClientProvider, { client }, children),
  });
  try {
    await waitFor(() => {
      assert.equal(hook.result.current.data?.tocs[0]?.generatedAt, 10);
    });
  } finally {
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    relayClient.fetchEvents = originalFetchEvents;
  }
});

test("cancellation prevents queued Wiki repository reads from starting", async () => {
  const { readWikiRepositoryProjection, unregisterWikiReadConsumer } =
    await import("./wikiReadCoordinator.ts");
  const expected = scope();
  const coordinates = ["one", "two", "three"].map(coordinate);
  const calls = [];
  let releaseReads;
  const readsReleased = new Promise((resolve) => {
    releaseReads = resolve;
  });
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return expected;
    if (command !== "wiki_snapshot_read") {
      throw new Error(`Unexpected Tauri command: ${command}`);
    }
    calls.push(args.coordinate);
    await readsReleased;
    return { token: expected, value: snapshot("missing") };
  });
  const controller = new AbortController();
  const consumerId = "cancellation-test";
  const projection = readWikiRepositoryProjection(
    expected,
    consumerId,
    coordinates,
    [],
    controller.signal,
  );
  try {
    while (calls.length < 2)
      await new Promise((resolve) => setTimeout(resolve, 0));
    controller.abort();
    releaseReads();
    await assert.rejects(projection, (error) => error?.name === "AbortError");
    assert.deepEqual(calls.sort(), coordinates.slice(0, 2).sort());
  } finally {
    releaseReads();
    unregisterWikiReadConsumer(expected, consumerId);
  }
});

test("last consumer retirement aborts work and drops late cache writes", async () => {
  const {
    clearWikiReadCoordinators,
    readWikiRepositoryProjection,
    registerWikiReadConsumer,
    unregisterWikiReadConsumer,
  } = await import("./wikiReadCoordinator.ts");
  const expected = scope();
  const coordinateValue = coordinate("retirement");
  const calls = [];
  let releaseRead;
  const readGate = new Promise((resolve) => {
    releaseRead = resolve;
  });
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return expected;
    if (command !== "wiki_snapshot_read") {
      throw new Error(`Unexpected Tauri command: ${command}`);
    }
    calls.push(args.coordinate);
    await readGate;
    return { token: expected, value: snapshot("complete") };
  });
  clearWikiReadCoordinators();
  const consumerId = "retirement-test";
  const projection = readWikiRepositoryProjection(
    expected,
    consumerId,
    [coordinateValue],
    [],
  );
  try {
    while (calls.length < 1)
      await new Promise((resolve) => setTimeout(resolve, 0));
    unregisterWikiReadConsumer(expected, consumerId);
    await Promise.resolve();
    releaseRead();
    await assert.rejects(projection, (error) => error?.name === "AbortError");

    // Re-register after the retirement microtask with no repositories. A late
    // completion from the retired read must not repopulate the new scope cache.
    registerWikiReadConsumer(expected, "replacement-consumer", [], []);
    const replacement = await readWikiRepositoryProjection(
      expected,
      "replacement-consumer",
      [],
      [],
    );
    assert.equal(replacement.size, 0);
  } finally {
    releaseRead();
    unregisterWikiReadConsumer(expected, "replacement-consumer");
    clearWikiReadCoordinators();
  }
});

test("the Wiki admission bound is global across distinct scopes", async () => {
  const { clearWikiReadCoordinators, readWikiRepositoryProjection } =
    await import("./wikiReadCoordinator.ts");
  const first = scope(11);
  const second = {
    ...scope(12),
    scope: { owner: "b".repeat(64), community: COMMUNITY },
  };
  const coordinates = [
    [coordinate("scope-a-one"), coordinate("scope-a-two")],
    [
      `30617:${second.scope.owner}:scope-b-one`,
      `30617:${second.scope.owner}:scope-b-two`,
    ],
  ];
  let active = 0;
  let peak = 0;
  let releaseReads;
  const readsReleased = new Promise((resolve) => {
    releaseReads = resolve;
  });
  let reachedTwo;
  const reachedTwoPromise = new Promise((resolve) => {
    reachedTwo = resolve;
  });
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return first;
    if (command !== "wiki_snapshot_read") {
      throw new Error(`Unexpected command: ${command}`);
    }
    active += 1;
    peak = Math.max(peak, active);
    if (active === 2) reachedTwo();
    await readsReleased;
    active -= 1;
    return { token: args.expected, value: snapshot("missing") };
  });
  clearWikiReadCoordinators();
  const firstAbort = new AbortController();
  const secondAbort = new AbortController();
  const firstRead = readWikiRepositoryProjection(
    first,
    "distinct-scope-a",
    coordinates[0],
    [],
    firstAbort.signal,
  );
  const secondRead = readWikiRepositoryProjection(
    second,
    "distinct-scope-b",
    coordinates[1],
    [],
    secondAbort.signal,
  );
  try {
    await reachedTwoPromise;
    firstAbort.abort();
    secondAbort.abort();
    releaseReads();
    await Promise.all([
      assert.rejects(firstRead, (error) => error?.name === "AbortError"),
      assert.rejects(secondRead, (error) => error?.name === "AbortError"),
    ]);
    assert.ok(peak <= 2, `global Wiki admission peak was ${peak}`);
  } finally {
    releaseReads();
    clearWikiReadCoordinators();
  }
});

test("equivalent fresh filter and priority arrays do not restart the canonical read", async () => {
  const { act, renderHook, waitFor, cleanup } = await import(
    "@testing-library/react"
  );
  const React = await import("react");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { relayClient } = await import("@/shared/api/relayClient");
  const { useWikiEventsQuery } = await import("./useWikiEventsQuery.ts");
  const expected = scope();
  const calls = [];
  const originalFetchEvents = relayClient.fetchEvents;
  installTauriInvoke(async (command, args) => {
    // A freshly allocated but equal token per capture: React Query's
    // structural sharing is what keeps the derived scope key stable.
    if (command === "owner_operation_scope") {
      return { ...expected, scope: { ...expected.scope } };
    }
    if (command === "wiki_snapshot_read") {
      calls.push(args.coordinate);
      return { token: expected, value: snapshot("complete") };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  });
  relayClient.fetchEvents = async () => [];
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const hook = renderHook(
    // Both arrays are newly allocated on every render, exactly as the library
    // and project screens build them.
    ({ repoD }) =>
      useWikiEventsQuery([repository(repoD)], {
        priorityCoordinates: [coordinate(repoD)],
      }),
    {
      initialProps: { repoD: "stable-first" },
      wrapper: ({ children }) =>
        React.createElement(QueryClientProvider, { client }, children),
    },
  );
  try {
    await waitFor(() => {
      assert.equal(
        hook.result.current.data?.repositoryStatuses[coordinate("stable-first")]
          ?.state,
        "ready",
      );
    });
    await waitFor(() => assert.equal(hook.result.current.isFetching, false));
    const settledCalls = calls.length;
    const projection = hook.result.current.data;

    await act(async () => {
      hook.rerender({ repoD: "stable-first" });
    });
    await act(async () => {
      hook.rerender({ repoD: "stable-first" });
    });

    // Equivalent renders must not rebuild the projection, retire and
    // re-register the shared consumer, or start another read.
    assert.equal(hook.result.current.data, projection);
    assert.equal(calls.length, settledCalls);

    // A genuinely different selection still updates the canonical read.
    await act(async () => {
      hook.rerender({ repoD: "stable-second" });
    });
    await waitFor(() => {
      assert.equal(
        hook.result.current.data?.repositoryStatuses[
          coordinate("stable-second")
        ]?.state,
        "ready",
      );
    });
    assert.ok(calls.includes(coordinate("stable-second")));
  } finally {
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    relayClient.fetchEvents = originalFetchEvents;
  }
});
