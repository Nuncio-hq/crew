import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { test } from "node:test";

const owner = "a".repeat(64);
const repoD = "crew";
const repoKey = `${owner}:${repoD}`;
const expected = {
  scope: { owner, community: "https://relay.example.test" },
  workspace_generation: 4,
  identity_generation: 9,
};

const stubs = new Map([
  [
    "@tanstack/react-query",
    "export const useMutation = x => x; export const useQueryClient = () => globalThis.wikiTest.query;",
  ],
  [
    "@tauri-apps/api/core",
    "export const invoke = (...args) => globalThis.wikiTest.invoke(...args);",
  ],
  [
    "@/features/wiki/hooks/useWikiEventsQuery",
    "export const wikiEventsQueryKey = ['crew-wiki-events'];",
  ],
  [
    "@/features/wiki/lib/wikiStore",
    "export const activateWikiJobScope = () => true; export const getWikiJobs = () => new Map(); export const setWikiJobScope = () => {}; export const setWikiJob = job => globalThis.wikiTest.jobs.push(job);",
  ],
]);

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/shared/api/ownerOperations")
      return {
        shortCircuit: true,
        url: new URL("../../../shared/api/ownerOperations.ts", import.meta.url)
          .href,
      };
    if (stubs.has(specifier))
      return { shortCircuit: true, url: `wiki-test:${specifier}` };
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url.startsWith("wiki-test:"))
      return {
        shortCircuit: true,
        format: "module",
        source: stubs.get(url.slice(10)),
      };
    return nextLoad(url, context);
  },
});

const { useWikiGenerate, wikiPublicationJobState } = await import(
  "./useWikiGenerate.ts"
);

function job(overrides = {}) {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    revision: 3,
    resourceKey: `30617:${owner}:${repoD}`,
    status: "reconciling",
    reconciled: false,
    snapshotId: "22222222-2222-4222-8222-222222222222",
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

function scoped(value, token = expected) {
  return { token, value };
}

function fixture({ prepare, dispatch, dispatchError } = {}) {
  const state = {
    expected,
    jobs: [],
    calls: [],
    invalidations: 0,
    query: {
      invalidateQueries: async () => {
        state.invalidations++;
      },
    },
    invoke: async (command, args) => {
      if (command === "owner_operation_scope") return state.expected;
      state.calls.push({ command, args });
      if (command === "wiki_publication_prepare") {
        return prepare || scoped({ result: "created", job: job() });
      }
      if (command === "wiki_publication_dispatch") {
        if (dispatchError) throw dispatchError;
        return (
          dispatch ||
          scoped(
            job({ status: "complete", reconciled: true, progress: "head" }),
          )
        );
      }
      throw new Error(`unexpected command: ${command}`);
    },
  };
  globalThis.wikiTest = state;
  return state;
}

const input = {
  owner,
  repoD,
  repoKey,
  repoPath: "/tmp/crew",
  workspaceMode: "folder",
  expectedScope: expected,
};

test("native generation prepares before dispatching the exact journal revision", async () => {
  const state = fixture();

  await useWikiGenerate().mutationFn(input);

  assert.deepEqual(
    state.calls.map(({ command }) => command),
    ["wiki_publication_prepare", "wiki_publication_dispatch"],
  );
  assert.equal(state.calls[0].args.workspaceMode, "folder");
  assert.equal(state.calls[0].args.coordinate, `30617:${owner}:${repoD}`);
  assert.equal(state.calls[1].args.id, job().id);
  assert.equal(state.calls[1].args.revision, job().revision);
  assert.equal(state.calls[1].args.explicitRetry, false);
  assert.equal(state.invalidations, 1);
  assert.equal(state.jobs.at(-1).status, "idle");
  assert.equal(state.jobs.at(-1).error, null);
});

test("an authoritative no-op does not dispatch or report a failure", async () => {
  const state = fixture({
    prepare: scoped({
      result: "noop",
      headId: "c".repeat(64),
      sourceRevision: `git:${"b".repeat(40)}`,
    }),
  });

  await useWikiGenerate().mutationFn(input);

  assert.deepEqual(
    state.calls.map(({ command }) => command),
    ["wiki_publication_prepare"],
  );
  assert.equal(state.invalidations, 1);
  assert.equal(state.jobs.at(-1).status, "idle");
  assert.equal(state.jobs.at(-1).error, null);
});

test("prepare failure leaves the production job failed and never dispatches", async () => {
  const state = fixture({ dispatchError: new Error("should not dispatch") });
  state.invoke = async (command, args) => {
    if (command === "owner_operation_scope") return state.expected;
    state.calls.push({ command, args });
    if (command === "wiki_publication_prepare")
      throw new Error("source workspace is unavailable");
    throw new Error("unexpected dispatch");
  };

  await assert.rejects(
    useWikiGenerate().mutationFn(input),
    /source workspace is unavailable/,
  );

  assert.deepEqual(
    state.calls.map(({ command }) => command),
    ["wiki_publication_prepare"],
  );
  assert.equal(state.jobs.at(-1).status, "failed");
  assert.equal(state.jobs.at(-1).error, "source workspace is unavailable");
});

test("dispatch errors remain visible after the durable prepare succeeds", async () => {
  const state = fixture({
    dispatch: scoped(
      job({
        status: "failed",
        lastError: "relay did not retain the exact Wiki event",
      }),
    ),
  });

  await assert.rejects(
    useWikiGenerate().mutationFn(input),
    /relay did not retain the exact Wiki event/,
  );

  assert.deepEqual(
    state.calls.map(({ command }) => command),
    ["wiki_publication_prepare", "wiki_publication_dispatch"],
  );
  assert.equal(state.jobs.at(-1).status, "failed");
  assert.equal(
    state.jobs.at(-1).error,
    "relay did not retain the exact Wiki event",
  );
  assert.equal(state.invalidations, 0);
});

test("a scope change rejects the returned operation instead of accepting stale state", async () => {
  const state = fixture({
    dispatch: scoped(job({ status: "complete", reconciled: true }), {
      ...expected,
      identity_generation: expected.identity_generation + 1,
    }),
  });

  await assert.rejects(
    useWikiGenerate().mutationFn(input),
    /active owner or community changed/,
  );

  assert.equal(state.jobs.at(-1).status, "failed");
  assert.match(state.jobs.at(-1).error, /active owner or community changed/);
  assert.equal(state.invalidations, 0);
});

test("a repository action is fenced to the scope captured with its rendered row", async () => {
  const state = fixture();
  const renderedScope = {
    ...expected,
    identity_generation: expected.identity_generation - 1,
  };

  await assert.rejects(
    useWikiGenerate().mutationFn({ ...input, expectedScope: renderedScope }),
    /active owner or community changed/,
  );
  assert.deepEqual(
    state.calls.map(({ command }) => command),
    [],
  );
  assert.equal(state.jobs.at(-1).status, "failed");
});

test("an A to B to A generation race keeps the new generation status fenced", async () => {
  const state = fixture();
  const scopeA = state.expected;
  const scopeB = {
    ...scopeA,
    identity_generation: scopeA.identity_generation + 1,
  };
  const scopeA2 = {
    ...scopeA,
    identity_generation: scopeA.identity_generation + 2,
  };
  let scopeCalls = 0;
  const invoke = state.invoke;
  state.invoke = async (command, args) => {
    if (command === "owner_operation_scope") {
      scopeCalls += 1;
      return [scopeA, scopeB, scopeA2][scopeCalls - 1] ?? scopeA2;
    }
    return invoke(command, args);
  };

  await assert.rejects(
    useWikiGenerate().mutationFn(input),
    /active owner or community changed/,
  );
  assert.equal(state.jobs.at(-1).status, "generating");
  assert.equal(state.invalidations, 0);
});

test("a reconciled dispatch result clears a prior native error", async () => {
  const state = fixture({
    dispatch: scoped(
      job({
        status: "complete",
        reconciled: true,
        progress: "head",
        lastError: "old transient failure",
      }),
    ),
  });

  await useWikiGenerate().mutationFn(input);

  assert.equal(state.jobs.at(-1).status, "idle");
  assert.equal(state.jobs.at(-1).error, null);
  assert.equal(state.invalidations, 1);
});

test("a reconciled superseded dispatch keeps the different-head-won warning", async () => {
  const state = fixture({
    dispatch: scoped(
      job({
        status: "superseded",
        reconciled: true,
        progress: "head",
        lastError: null,
      }),
    ),
  });

  await useWikiGenerate().mutationFn(input);

  const final = state.jobs.at(-1);
  // Reconciled, but a different head won: the card must keep the warning and
  // its fresh Generate affordance instead of rendering as complete/idle.
  assert.equal(final.status, "failed");
  assert.equal(final.nativeStatus, "superseded");
  assert.equal(final.reconciled, true);
  assert.equal(final.operationId, job().id);
  assert.equal(final.operationRevision, job().revision);
  assert.equal(final.error, null);
  // A superseded head is not the typed immutable-retired Regenerate case.
  assert.equal(final.retiredDependencyId, undefined);
  assert.equal(state.invalidations, 1);
});

test("typed immutable-retired proof remains visible for explicit regeneration", () => {
  const retiredDependencyId = "d".repeat(64);
  const projected = wikiPublicationJobState(
    job({ retiredDependencyId }),
    expected,
  );

  assert.equal(projected?.retiredDependencyId, retiredDependencyId);
  assert.equal(projected?.operationId, job().id);
  assert.equal(projected?.operationRevision, job().revision);
  assert.equal(projected?.reconciled, false);
});
