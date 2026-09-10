import assert from "node:assert/strict";
import { after, test } from "node:test";
import { JSDOM } from "jsdom";

const owner = "a".repeat(64);
const community = "https://relay.example.test";
const repoKeyValue = `${owner}:crew`;
const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});

const { act, cleanup, renderHook, waitFor } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const {
  getWikiActiveScope,
  getWikiJobs,
  getWikiStoreGeneration,
  resetWikiStore,
  setWikiJob,
  setWikiJobScope,
} = await import("@/features/wiki/lib/wikiStore");
const { wikiPublicationJobState } = await import("./useWikiGenerate.ts");
const { useWikiPublicationRecovery, wikiPublicationQueryKey } = await import(
  "./useWikiPublicationRecovery.ts"
);

after(() => dom.window.close());

function scope(identityGeneration) {
  return {
    scope: { owner, community },
    workspace_generation: 4,
    identity_generation: identityGeneration,
  };
}

function nativeJob(overrides = {}) {
  return {
    id: "operation-1",
    revision: 1,
    resourceKey: `30617:${owner}:crew`,
    status: "reconciling",
    reconciled: false,
    snapshotId: "snapshot-1",
    sourceRevision: `git:${"b".repeat(40)}`,
    cadence: "manual",
    pages: 2,
    attempts: 1,
    progress: "preparing",
    headAttempted: false,
    cancelRequested: false,
    reconcileOnly: false,
    retiredDependencyId: null,
    retryAt: 0,
    lastError: null,
    ...overrides,
  };
}

function installTauriInvoke(handler) {
  const bridge = { invoke: handler, transformCallback: () => 1 };
  globalThis.__TAURI_INTERNALS__ = bridge;
  dom.window.__TAURI_INTERNALS__ = bridge;
}

/** Claim and project a row the way the production store APIs do. */
function seedRow(job, at) {
  setWikiJobScope(repoKeyValue, at);
  setWikiJob(wikiPublicationJobState(job, at));
}

function testClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      // MutationCache.clear() drops mutations without destroying them, so a
      // settled mutation keeps its garbage-collection timeout armed — five
      // minutes by default, which holds the node event loop open long after
      // the suite finishes. QueryCache.clear() destroys queries, so only the
      // mutation side needs this; the asserted query cache behaviour is
      // unchanged.
      mutations: { gcTime: 0 },
    },
  });
}

function mount(client, at) {
  return renderHook(() => useWikiPublicationRecovery(at), {
    wrapper: ({ children }) =>
      React.createElement(QueryClientProvider, { client }, children),
  });
}

/** Drain pending microtasks and the timer turn React Query schedules on. */
async function flush() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function recover(hook, input) {
  let outcome;
  await act(async () => {
    outcome = await hook.result.current.recoverAsync(input).then(
      (value) => ({ value }),
      (error) => ({ error }),
    );
  });
  return outcome;
}

function spyInvalidations(client) {
  const invalidations = [];
  const original = client.invalidateQueries.bind(client);
  client.invalidateQueries = async (filters) => {
    invalidations.push(filters?.queryKey?.[0]);
    return original(filters);
  };
  return invalidations;
}

function statusQueries(client) {
  return client.getQueryCache().findAll({ queryKey: wikiPublicationQueryKey });
}

test("a deferred status list adopts no scope before the effect revalidates", async () => {
  resetWikiStore();
  const a1 = scope(9);
  const a2 = scope(10);
  const a3 = scope(11);
  // The renderer store still holds A1 while the rendered scope and native
  // identity have already moved ahead of it.
  seedRow(nativeJob({ id: "a1-operation", revision: 2 }), a1);
  let native = a2;
  let scopeCalls = 0;
  const listArgs = [];
  let releaseList;
  const listGate = new Promise((resolve) => {
    releaseList = resolve;
  });
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") {
      scopeCalls += 1;
      return native;
    }
    if (command === "wiki_publication_list") {
      listArgs.push(args.expected);
      await listGate;
      // Another window moves native identity forward while the list is in
      // flight, so the returned token is already stale on arrival.
      native = a3;
      return {
        token: a2,
        value: [nativeJob({ id: "a2-operation", revision: 5 })],
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const hook = mount(client, a2);
  try {
    await waitFor(() => assert.equal(listArgs.length, 1));
    assert.deepEqual(listArgs[0], a2);
    const generation = getWikiStoreGeneration();
    releaseList();
    await flush();
    await waitFor(() => assert.ok(scopeCalls >= 1));
    await flush();

    // The query function is pure: no adoption, no clearing. The A1 rows and
    // claims survive a list whose token the effect then refuses to hydrate.
    assert.equal(getWikiStoreGeneration(), generation);
    assert.deepEqual(getWikiActiveScope(), a1);
    assert.equal(getWikiJobs().get(repoKeyValue)?.operationId, "a1-operation");
    assert.equal(getWikiJobs().size, 1);
    assert.equal(hook.result.current.error, null);
  } finally {
    releaseList();
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("two recovery mounts share one canonical status query retired on the last unmount", async () => {
  resetWikiStore();
  const current = scope(12);
  let listCalls = 0;
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return current;
    if (command === "wiki_publication_list") {
      listCalls += 1;
      return {
        token: current,
        value: [nativeJob({ id: "listed-operation", revision: 3 })],
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const first = mount(client, current);
  const second = mount(client, current);
  try {
    // Adoption still happens — in the effect, after a fresh native capture.
    await waitFor(() =>
      assert.equal(
        getWikiJobs().get(repoKeyValue)?.operationId,
        "listed-operation",
      ),
    );
    assert.deepEqual(getWikiActiveScope(), current);
    assert.equal(listCalls, 1);
    assert.equal(statusQueries(client).length, 1);

    first.unmount();
    await flush();
    assert.equal(statusQueries(client).length, 1);

    second.unmount();
    await flush();
    assert.equal(statusQueries(client).length, 0);
  } finally {
    first.unmount();
    second.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("explicit regeneration passes the displayed scope and fresh source selection", async () => {
  resetWikiStore();
  const current = scope(13);
  const predecessor = nativeJob({
    id: "retired-operation",
    revision: 8,
    retiredDependencyId: "d".repeat(64),
  });
  const successor = nativeJob({
    id: "new-successor",
    revision: 0,
    status: "preparing",
    snapshotId: "fresh-snapshot",
    sourceRevision: "git:fresh",
  });
  const dispatched = nativeJob({
    id: "new-successor",
    revision: 1,
    status: "complete",
    reconciled: true,
    progress: "head",
    snapshotId: "fresh-snapshot",
    sourceRevision: "git:fresh",
  });
  seedRow(predecessor, current);
  let listRows = [predecessor];
  const calls = [];
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return current;
    if (command === "wiki_publication_list") {
      return { token: current, value: listRows };
    }
    calls.push({ command, args });
    if (command === "wiki_publication_regenerate") {
      listRows = [successor];
      return { token: current, value: { result: "created", job: successor } };
    }
    if (command === "wiki_publication_dispatch") {
      listRows = [dispatched];
      return { token: current, value: dispatched };
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const invalidations = spyInvalidations(client);
  const hook = mount(client, current);
  try {
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: current,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
      retiredDependencyId: "d".repeat(64),
    });

    assert.equal(outcome.error, undefined);
    assert.deepEqual(
      calls.map(({ command }) => command),
      ["wiki_publication_regenerate", "wiki_publication_dispatch"],
    );
    assert.deepEqual(calls[0].args, {
      expected: current,
      id: "retired-operation",
      revision: 8,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
    });
    assert.deepEqual(calls[1].args, {
      expected: current,
      id: "new-successor",
      revision: 0,
      explicitRetry: false,
    });
    const row = getWikiJobs().get(repoKeyValue);
    assert.equal(row?.operationId, "new-successor");
    assert.equal(row?.operationRevision, 1);
    assert.equal(row?.status, "idle");
    assert.ok(invalidations.includes("crew-wiki-events"));
  } finally {
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("regeneration rejects a row without typed immutable-retired proof", async () => {
  resetWikiStore();
  const current = scope(14);
  const predecessor = nativeJob({ id: "retired-operation", revision: 8 });
  seedRow(predecessor, current);
  const calls = [];
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return current;
    if (command === "wiki_publication_list") {
      return { token: current, value: [predecessor] };
    }
    calls.push(command);
    throw new Error("regenerate must not reach native without proof");
  });
  const client = testClient();
  const hook = mount(client, current);
  try {
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: current,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
    });

    assert.match(
      outcome.error?.message ?? "",
      /immutable-dependency retirement proof/,
    );
    assert.deepEqual(calls, []);
  } finally {
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("a scope change during regeneration writes no successor row and invalidates nothing", async () => {
  resetWikiStore();
  const a1 = scope(15);
  const a2 = scope(16);
  const predecessor = nativeJob({
    id: "retired-operation",
    revision: 8,
    retiredDependencyId: "d".repeat(64),
  });
  seedRow(predecessor, a1);
  let native = a1;
  const calls = [];
  let releaseList;
  const listGate = new Promise((resolve) => {
    releaseList = resolve;
  });
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return native;
    if (command === "wiki_publication_list") {
      await listGate;
      return { token: a1, value: [predecessor] };
    }
    calls.push({ command, args });
    if (command === "wiki_publication_regenerate") {
      // Another window changes identity while regenerate runs. The returned
      // token still proves only the scope the request ran under.
      native = a2;
      return {
        token: a1,
        value: {
          result: "created",
          job: nativeJob({ id: "new-successor", revision: 0 }),
        },
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const invalidations = spyInvalidations(client);
  const hook = mount(client, a1);
  try {
    const generation = getWikiStoreGeneration();
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: a1,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
      retiredDependencyId: "d".repeat(64),
    });
    await flush();

    assert.match(outcome.error?.message ?? "", /active owner or community/);
    assert.deepEqual(
      calls.map(({ command }) => command),
      ["wiki_publication_regenerate"],
    );
    // No prepared projection, no failure projection, no content invalidation.
    assert.equal(getWikiStoreGeneration(), generation);
    assert.equal(
      getWikiJobs().get(repoKeyValue)?.operationId,
      "retired-operation",
    );
    assert.equal(getWikiJobs().get(repoKeyValue)?.operationRevision, 8);
    assert.deepEqual(invalidations, []);
  } finally {
    releaseList();
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("a regeneration dispatch failure keeps the known successor identity", async () => {
  resetWikiStore();
  const current = scope(17);
  const predecessor = nativeJob({ id: "retired-operation", revision: 8 });
  seedRow(predecessor, current);
  const calls = [];
  let releaseList;
  const listGate = new Promise((resolve) => {
    releaseList = resolve;
  });
  installTauriInvoke(async (command, args) => {
    if (command === "owner_operation_scope") return current;
    if (command === "wiki_publication_list") {
      await listGate;
      return { token: current, value: [predecessor] };
    }
    calls.push({ command, args });
    if (command === "wiki_publication_regenerate") {
      return {
        token: current,
        value: {
          result: "created",
          job: nativeJob({
            id: "known-successor",
            revision: 4,
            status: "preparing",
          }),
        },
      };
    }
    if (command === "wiki_publication_dispatch") {
      throw new Error("successor dispatch failed");
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const hook = mount(client, current);
  try {
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: current,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
      retiredDependencyId: "d".repeat(64),
    });

    assert.match(outcome.error?.message ?? "", /successor dispatch failed/);
    // Recovery's first status projection bootstraps media URL helpers after
    // dispatch fails. Keep those expected infrastructure calls out of the
    // operation ordering assertion while still failing on any other command.
    const operationCalls = calls.filter(
      ({ command }) =>
        command !== "get_relay_http_url" && command !== "get_media_proxy_port",
    );
    assert.deepEqual(
      operationCalls.map(({ command }) => command),
      ["wiki_publication_regenerate", "wiki_publication_dispatch"],
    );
    const row = getWikiJobs().get(repoKeyValue);
    assert.equal(row?.operationId, "known-successor");
    assert.equal(row?.operationRevision, 4);
    assert.equal(row?.status, "failed");
    assert.equal(row?.error, "successor dispatch failed");
  } finally {
    releaseList();
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("a stale dispatch failure never demotes a newer completed successor revision", async () => {
  resetWikiStore();
  const current = scope(18);
  const predecessor = nativeJob({ id: "retired-operation", revision: 8 });
  seedRow(predecessor, current);
  let releaseList;
  const listGate = new Promise((resolve) => {
    releaseList = resolve;
  });
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return current;
    if (command === "wiki_publication_list") {
      await listGate;
      return { token: current, value: [predecessor] };
    }
    if (command === "wiki_publication_regenerate") {
      return {
        token: current,
        value: {
          result: "created",
          job: nativeJob({
            id: "known-successor",
            revision: 4,
            status: "preparing",
          }),
        },
      };
    }
    if (command === "wiki_publication_dispatch") {
      // A status poll completes the same successor at a newer revision while
      // this dispatch is still in flight; the dispatch then fails on the wire.
      setWikiJob(
        wikiPublicationJobState(
          nativeJob({
            id: "known-successor",
            revision: 6,
            status: "complete",
            reconciled: true,
            progress: "head",
            lastError: null,
          }),
          current,
        ),
      );
      throw new Error("successor dispatch failed");
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const hook = mount(client, current);
  try {
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: current,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
      retiredDependencyId: "d".repeat(64),
    });

    // The error still reaches the user through the mutation, but the newer
    // native projection is authoritative and stays complete.
    assert.match(outcome.error?.message ?? "", /successor dispatch failed/);
    const row = getWikiJobs().get(repoKeyValue);
    assert.equal(row?.operationId, "known-successor");
    assert.equal(row?.operationRevision, 6);
    assert.equal(row?.nativeStatus, "complete");
    assert.equal(row?.reconciled, true);
    assert.equal(row?.status, "idle");
    assert.equal(row?.error, null);
  } finally {
    releaseList();
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("a delayed regeneration response never overwrites a different newer operation", async () => {
  resetWikiStore();
  const current = scope(21);
  const predecessor = nativeJob({ id: "retired-operation", revision: 8 });
  seedRow(predecessor, current);
  const laterOperation = nativeJob({ id: "later-operation", revision: 2 });
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return current;
    if (command === "wiki_publication_list") {
      return { token: current, value: [predecessor] };
    }
    if (command === "wiki_publication_regenerate") {
      // A poll hydrates an unrelated newer operation for this repository
      // while the regenerate IPC is still outstanding.
      setWikiJob(wikiPublicationJobState(laterOperation, current));
      return {
        token: current,
        value: {
          result: "created",
          job: nativeJob({
            id: "known-successor",
            revision: 4,
            status: "preparing",
          }),
        },
      };
    }
    if (command === "wiki_publication_dispatch") {
      return {
        token: current,
        value: nativeJob({
          id: "known-successor",
          revision: 5,
          status: "complete",
          reconciled: true,
          progress: "head",
        }),
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const hook = mount(client, current);
  try {
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: current,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
      retiredDependencyId: "d".repeat(64),
    });
    await flush();

    assert.equal(outcome.error, undefined);
    // Neither the prepared successor nor the final response may replace an
    // operation this callback never saw.
    const row = getWikiJobs().get(repoKeyValue);
    assert.equal(row?.operationId, "later-operation");
    assert.equal(row?.operationRevision, 2);
    assert.equal(row?.status, "generating");
  } finally {
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("a delayed dispatch response never overwrites a different newer operation", async () => {
  resetWikiStore();
  const current = scope(22);
  const predecessor = nativeJob({ id: "retired-operation", revision: 8 });
  seedRow(predecessor, current);
  const laterOperation = nativeJob({ id: "later-operation", revision: 2 });
  const preparedRows = [];
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return current;
    if (command === "wiki_publication_list") {
      return { token: current, value: [predecessor] };
    }
    if (command === "wiki_publication_regenerate") {
      return {
        token: current,
        value: {
          result: "created",
          job: nativeJob({
            id: "known-successor",
            revision: 4,
            status: "preparing",
          }),
        },
      };
    }
    if (command === "wiki_publication_dispatch") {
      // The prepared successor did reach the store; an unrelated newer
      // operation then replaces it while the dispatch is outstanding.
      preparedRows.push(getWikiJobs().get(repoKeyValue)?.operationId);
      setWikiJob(wikiPublicationJobState(laterOperation, current));
      return {
        token: current,
        value: nativeJob({
          id: "known-successor",
          revision: 5,
          status: "complete",
          reconciled: true,
          progress: "head",
        }),
      };
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const hook = mount(client, current);
  try {
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: current,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
      retiredDependencyId: "d".repeat(64),
    });
    await flush();

    assert.equal(outcome.error, undefined);
    assert.deepEqual(preparedRows, ["known-successor"]);
    const row = getWikiJobs().get(repoKeyValue);
    assert.equal(row?.operationId, "later-operation");
    assert.equal(row?.operationRevision, 2);
    assert.equal(row?.status, "generating");
  } finally {
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});

test("a scope change during successor dispatch projects no failure", async () => {
  resetWikiStore();
  const a1 = scope(19);
  const a2 = scope(20);
  const predecessor = nativeJob({ id: "retired-operation", revision: 8 });
  seedRow(predecessor, a1);
  let native = a1;
  let releaseList;
  const listGate = new Promise((resolve) => {
    releaseList = resolve;
  });
  installTauriInvoke(async (command) => {
    if (command === "owner_operation_scope") return native;
    if (command === "wiki_publication_list") {
      await listGate;
      return { token: a1, value: [predecessor] };
    }
    if (command === "wiki_publication_regenerate") {
      return {
        token: a1,
        value: {
          result: "created",
          job: nativeJob({
            id: "known-successor",
            revision: 4,
            status: "preparing",
          }),
        },
      };
    }
    if (command === "wiki_publication_dispatch") {
      native = a2;
      throw new Error("successor dispatch failed");
    }
    throw new Error(`unexpected command: ${command}`);
  });
  const client = testClient();
  const invalidations = spyInvalidations(client);
  const hook = mount(client, a1);
  try {
    const outcome = await recover(hook, {
      action: "regenerate",
      repoKey: repoKeyValue,
      operationId: "retired-operation",
      operationRevision: 8,
      expectedScope: a1,
      repoPath: "/tmp/crew",
      workspaceMode: "folder",
      retiredDependencyId: "d".repeat(64),
    });
    await flush();

    assert.match(outcome.error?.message ?? "", /successor dispatch failed/);
    // The prepared successor stays exactly as written under A1: a failure
    // belonging to a scope this window no longer owns is not projected.
    const row = getWikiJobs().get(repoKeyValue);
    assert.equal(row?.operationId, "known-successor");
    assert.equal(row?.operationRevision, 4);
    assert.equal(row?.status, "generating");
    assert.equal(row?.error, null);
    assert.deepEqual(invalidations, []);
  } finally {
    releaseList();
    hook.unmount();
    client.clear();
    client.unmount();
    cleanup();
    resetWikiStore();
  }
});
