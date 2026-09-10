import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { test } from "node:test";

const stubs = new Map([
  ["@tauri-apps/api/core", "export const invoke = async () => {};"],
]);
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/shared/api/ownerOperations") {
      return {
        shortCircuit: true,
        url: new URL("../../../shared/api/ownerOperations.ts", import.meta.url)
          .href,
      };
    }
    if (specifier === "@/shared/constants/kinds") {
      return {
        shortCircuit: true,
        url: new URL("../../../shared/constants/kinds.ts", import.meta.url)
          .href,
      };
    }
    if (stubs.has(specifier))
      return { shortCircuit: true, url: `wiki-store-test:${specifier}` };
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url.startsWith("wiki-store-test:")) {
      return {
        shortCircuit: true,
        format: "module",
        source: stubs.get(url.slice("wiki-store-test:".length)),
      };
    }
    return nextLoad(url, context);
  },
});

const {
  getWikiJobs,
  getWikiStoreGeneration,
  hydrateWikiJobs,
  activateWikiJobScope,
  resetWikiStore,
  setWikiJob,
  setWikiJobScope,
} = await import("./wikiStore.ts");
const { jobForRepo } = await import("./wikiEvents.ts");

const scope = (owner, identity_generation) => ({
  scope: { owner, community: "https://relay.example.test" },
  workspace_generation: 4,
  identity_generation,
});
const job = (
  repoKey,
  capturedScope,
  error,
  operationId,
  operationRevision,
) => ({
  repoKey,
  scope: capturedScope,
  operationId,
  operationRevision,
  status: "failed",
  done: 0,
  total: 1,
  error,
  costNote: null,
});

test("an equal-generation owner switch is rejected instead of mixing jobs", () => {
  resetWikiStore();
  const ownerA = "a".repeat(64);
  const ownerB = "b".repeat(64);
  const scopeA = scope(ownerA, 1);
  const scopeB = scope(ownerB, 1);
  setWikiJobScope(`${ownerA}:crew`, scopeA);
  setWikiJob(job(`${ownerA}:crew`, scopeA, "owner A"));
  setWikiJobScope(`${ownerB}:crew`, scopeB);
  setWikiJob(job(`${ownerB}:crew`, scopeB, "owner B"));

  const jobs = getWikiJobs();
  assert.equal(jobForRepo(jobs, ownerA, "crew")?.error, "owner A");
  assert.equal(jobForRepo(jobs, ownerB, "crew"), undefined);
});

test("a validated newer scope replaces the active scope before hydration", () => {
  resetWikiStore();
  const owner = "3".repeat(64);
  const repo = `${owner}:crew`;
  const scopeA = scope(owner, 1);
  const scopeA2 = scope(owner, 2);
  setWikiJobScope(repo, scopeA);
  setWikiJob(job(repo, scopeA, "old A"));

  assert.equal(activateWikiJobScope(scopeA2), true);
  hydrateWikiJobs([job(repo, scopeA2, "current A2", "a2", 1)], scopeA2);

  assert.equal(getWikiJobs().get(repo)?.error, "current A2");
  assert.equal(getWikiJobs().get(repo)?.scope?.identity_generation, 2);
});

test("A to B to A scope changes reject stale completions and clear old jobs", () => {
  resetWikiStore();
  const owner = "a".repeat(64);
  const repo = `${owner}:crew`;
  const scopeA = scope(owner, 1);
  const scopeB = scope(owner, 2);
  const scopeA2 = scope(owner, 3);

  setWikiJobScope(repo, scopeA);
  setWikiJob(job(repo, scopeA, "generation A"));
  setWikiJobScope(repo, scopeB);
  setWikiJob(job(repo, scopeA, "stale A"));
  assert.equal(getWikiJobs().get(repo), undefined);
  setWikiJob(job(repo, scopeB, "generation B"));
  setWikiJobScope(repo, scopeA2);
  assert.equal(getWikiJobs().get(repo), undefined);
  setWikiJob(job(repo, scopeA2, "generation A2"));
  assert.equal(getWikiJobs().get(repo)?.error, "generation A2");
});

test("a late initial list cannot clobber a newer local operation revision", () => {
  resetWikiStore();
  const owner = "c".repeat(64);
  const repo = `${owner}:crew`;
  const capturedScope = scope(owner, 1);
  setWikiJobScope(repo, capturedScope);
  setWikiJob(
    job(
      repo,
      capturedScope,
      "newer local state",
      "33333333-3333-4333-8333-333333333333",
      4,
    ),
  );

  // A list response from before the local CAS is represented by a lower
  // revision for the same operation; the store keeps the newer projection.
  hydrateWikiJobs(
    [
      job(
        repo,
        capturedScope,
        "older list state",
        "33333333-3333-4333-8333-333333333333",
        3,
      ),
    ],
    capturedScope,
  );
  assert.equal(getWikiJobs().get(repo)?.operationRevision, 4);
});

test("a pre-prepare empty list cannot erase a newer local pending job", () => {
  resetWikiStore();
  const owner = "d".repeat(64);
  const repo = `${owner}:crew`;
  const capturedScope = scope(owner, 1);
  setWikiJobScope(repo, capturedScope);
  const startedGeneration = getWikiStoreGeneration();

  setWikiJob({
    ...job(
      repo,
      capturedScope,
      null,
      "44444444-4444-4444-8444-444444444444",
      1,
    ),
    status: "generating",
    operationId: undefined,
    operationRevision: undefined,
  });
  hydrateWikiJobs([], capturedScope, startedGeneration);

  assert.equal(getWikiJobs().get(repo)?.status, "generating");
});

test("a late list cannot overwrite a new scope generation with the same operation id", () => {
  resetWikiStore();
  const owner = "e".repeat(64);
  const repo = `${owner}:crew`;
  const scopeA2 = scope(owner, 2);
  const operationId = "55555555-5555-4555-8555-555555555555";

  setWikiJobScope(repo, scopeA2);
  const startedGeneration = getWikiStoreGeneration();
  setWikiJob(job(repo, scopeA2, "new A generation", operationId, 7));

  hydrateWikiJobs(
    [job(repo, scopeA2, "late old A", operationId, 7)],
    scopeA2,
    startedGeneration,
  );

  assert.equal(getWikiJobs().get(repo)?.error, "new A generation");
  assert.equal(getWikiJobs().get(repo)?.scope?.identity_generation, 2);
});

test("a stale scope hydration cannot erase the newer scope job or claim", () => {
  resetWikiStore();
  const owner = "f".repeat(64);
  const repo = `${owner}:crew`;
  const scopeA = scope(owner, 1);
  const scopeA2 = scope(owner, 2);

  setWikiJobScope(repo, scopeA2);
  setWikiJob(
    job(
      repo,
      scopeA2,
      "new generation",
      "66666666-6666-4666-8666-666666666666",
      2,
    ),
  );
  hydrateWikiJobs(
    [
      job(
        repo,
        scopeA,
        "late old generation",
        "66666666-6666-4666-8666-666666666666",
        2,
      ),
    ],
    scopeA,
  );

  assert.equal(getWikiJobs().get(repo)?.error, "new generation");
  assert.equal(getWikiJobs().get(repo)?.scope?.identity_generation, 2);
});

test("an empty stale scope hydration preserves a prepared scope claim without a job", () => {
  resetWikiStore();
  const owner = "1".repeat(64);
  const repo = `${owner}:crew`;
  const scopeA = scope(owner, 1);
  const scopeA2 = scope(owner, 2);

  setWikiJobScope(repo, scopeA2);
  hydrateWikiJobs([], scopeA);
  setWikiJob(job(repo, scopeA2, "new prepare", undefined, undefined));

  assert.equal(getWikiJobs().get(repo)?.error, "new prepare");
  assert.equal(getWikiJobs().get(repo)?.scope?.identity_generation, 2);
});

test("a late callback cannot regress the same operation to an older revision", () => {
  resetWikiStore();
  const owner = "5".repeat(64);
  const repo = `${owner}:crew`;
  const capturedScope = scope(owner, 1);
  const operationId = "88888888-8888-4888-8888-888888888888";

  setWikiJobScope(repo, capturedScope);
  setWikiJob({
    ...job(repo, capturedScope, null, operationId, 6),
    status: "idle",
  });

  // A generation error callback still holding the revision it dispatched must
  // not reinstate that older projection over the newer journal row.
  setWikiJob(job(repo, capturedScope, "stale dispatch error", operationId, 4));
  assert.equal(getWikiJobs().get(repo)?.operationRevision, 6);
  assert.equal(getWikiJobs().get(repo)?.status, "idle");
  assert.equal(getWikiJobs().get(repo)?.error, null);

  // The revision that is current may still publish its own outcome, and a
  // newer revision replaces the row.
  setWikiJob(
    job(repo, capturedScope, "current revision failed", operationId, 6),
  );
  assert.equal(getWikiJobs().get(repo)?.error, "current revision failed");
  setWikiJob(job(repo, capturedScope, "newer revision", operationId, 7));
  assert.equal(getWikiJobs().get(repo)?.operationRevision, 7);
  assert.equal(getWikiJobs().get(repo)?.error, "newer revision");
});

test("a different operation may still replace a row without an old revision fence", () => {
  resetWikiStore();
  const owner = "6".repeat(64);
  const repo = `${owner}:crew`;
  const capturedScope = scope(owner, 1);

  setWikiJobScope(repo, capturedScope);
  setWikiJob(job(repo, capturedScope, "predecessor", "operation-old", 9));
  setWikiJob(job(repo, capturedScope, "successor", "operation-new", 0));

  assert.equal(getWikiJobs().get(repo)?.operationId, "operation-new");
  assert.equal(getWikiJobs().get(repo)?.operationRevision, 0);
});

test("an old A empty-list hydration cannot erase a current A2 claim or job", () => {
  resetWikiStore();
  const owner = "2".repeat(64);
  const repo = `${owner}:crew`;
  const scopeA = scope(owner, 1);
  const scopeA2 = scope(owner, 2);

  setWikiJobScope(repo, scopeA2);
  setWikiJob({
    ...job(
      repo,
      scopeA2,
      "current A2",
      "77777777-7777-4777-8777-777777777777",
      4,
    ),
    status: "generating",
    error: null,
  });
  hydrateWikiJobs([], scopeA, getWikiStoreGeneration() - 1);

  assert.equal(getWikiJobs().get(repo)?.error, null);
  assert.equal(getWikiJobs().get(repo)?.scope?.identity_generation, 2);
});
