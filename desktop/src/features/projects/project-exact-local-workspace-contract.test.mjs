import assert from "node:assert/strict";
import { test } from "node:test";

import {
  localWorkspaceSourceState,
  readExactLocalWorkspaceSnapshot,
  readProjectLocalRepoSnapshot,
} from "./lib/project-exact-local-workspace.ts";

const LOCAL_PATH = "/Users/oscar/Projects/Nùncio Crew";

function snapshot(path = LOCAL_PATH) {
  return {
    path,
    snapshot: {
      commits: [],
      contributors: [],
      files: [],
      latestCommit: null,
    },
  };
}

function dependencies(overrides = {}) {
  const calls = [];
  return {
    calls,
    dependencies: {
      basename: async () => "Nùncio Crew",
      dirname: async () => "/Users/oscar/Projects",
      getLocalSnapshot: async (input) => {
        calls.push(input);
        return snapshot();
      },
      normalize: async (path) => path,
      ...overrides,
    },
  };
}

test("exact workspace reading uses folder basename rather than Project d-tag", async () => {
  const harness = dependencies();

  const result = await readExactLocalWorkspaceSnapshot(
    {
      baseBranch: "main",
      defaultBranch: "main",
      localWorkspacePath: LOCAL_PATH,
      projectDtag: "renamed-project",
    },
    harness.dependencies,
  );

  assert.equal(result?.path, LOCAL_PATH);
  assert.deepEqual(harness.calls, [
    {
      baseBranch: "main",
      cloneUrl: null,
      defaultBranch: "main",
      projectDtag: "Nùncio Crew",
      reposDir: "/Users/oscar/Projects",
    },
  ]);
});

test("missing Git checkout stays unavailable without a fallback read", async () => {
  const harness = dependencies({
    getLocalSnapshot: async (input) => {
      harness.calls.push(input);
      return null;
    },
  });

  assert.equal(
    await readExactLocalWorkspaceSnapshot(
      {
        defaultBranch: "main",
        localWorkspacePath: LOCAL_PATH,
        projectDtag: "crew",
      },
      harness.dependencies,
    ),
    null,
  );
  assert.equal(harness.calls.length, 1);
});

test("a mismatched resolved path fails closed", async () => {
  const harness = dependencies({
    getLocalSnapshot: async () =>
      snapshot("/Users/oscar/Projects/another-crew"),
  });

  await assert.rejects(
    readExactLocalWorkspaceSnapshot(
      {
        defaultBranch: "main",
        localWorkspacePath: LOCAL_PATH,
        projectDtag: "crew",
      },
      harness.dependencies,
    ),
    /different folder/,
  );
});

test("ordinary Buzz checkouts preserve clone-origin matching", async () => {
  const harness = dependencies();

  await readProjectLocalRepoSnapshot(
    {
      cloneUrl: "https://relay.example/git/owner/crew",
      defaultBranch: "main",
      localWorkspacePath: null,
      projectDtag: "crew",
      reposDir: "/Users/oscar/Projects",
    },
    harness.dependencies,
  );

  assert.deepEqual(harness.calls[0], {
    baseBranch: undefined,
    cloneUrl: "https://relay.example/git/owner/crew",
    defaultBranch: "main",
    projectDtag: "crew",
    reposDir: "/Users/oscar/Projects",
  });
});

test("invalid local metadata never falls back to a managed checkout", async () => {
  const harness = dependencies();

  assert.equal(
    await readProjectLocalRepoSnapshot(
      {
        cloneUrl: null,
        defaultBranch: "main",
        localWorkspacePath: null,
        localWorkspaceStatus: "invalid",
        projectDtag: "crew",
        reposDir: "/Users/oscar/Projects",
      },
      harness.dependencies,
    ),
    null,
  );
  assert.equal(harness.calls.length, 0);
});

test("linked Local labels distinguish checking, ready, and unavailable", () => {
  assert.deepEqual(
    localWorkspaceSourceState({
      hasSnapshot: false,
      isError: false,
      isLinked: true,
      isLoading: true,
    }),
    { disabled: false, label: "Local checking" },
  );
  assert.deepEqual(
    localWorkspaceSourceState({
      hasSnapshot: true,
      isError: false,
      isLinked: true,
      isLoading: false,
    }),
    { disabled: false, label: "Local" },
  );
  assert.deepEqual(
    localWorkspaceSourceState({
      hasSnapshot: false,
      isError: true,
      isLinked: true,
      isLoading: false,
    }),
    { disabled: true, label: "Local unavailable" },
  );
});
