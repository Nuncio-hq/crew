import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  localWorkspaceSourceState,
  readProjectLocalRepoSnapshot,
} from "./lib/project-exact-local-workspace.ts";
import { isProjectLocal } from "./lib/projectLocalRepos.ts";

const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";

function snapshot(path) {
  return {
    path,
    snapshot: {
      commits: [],
      contributors: [],
      files: [{ path: "README.md", type: "file" }],
      latestCommit: null,
    },
  };
}

function dependencies(getLocalSnapshot) {
  const calls = [];
  return {
    calls,
    dependencies: {
      basename: async () => "Nuncio Crew",
      dirname: async () => "/Users/oscar/Projects",
      getLocalSnapshot: async (input) => {
        calls.push(input);
        return getLocalSnapshot
          ? getLocalSnapshot(input)
          : snapshot(LOCAL_PATH);
      },
      normalize: async (path) => path,
    },
  };
}

function repository(overrides = {}) {
  return {
    cloneUrls: [],
    dtag: "nuncio-crew",
    localWorkspacePath: null,
    localWorkspaceStatus: "unlinked",
    ...overrides,
  };
}

test("a managed read tries every advertised clone URL before giving up", async () => {
  const harness = dependencies((input) =>
    input.cloneUrl === "https://github.com/Nuncio-hq/crew.git"
      ? snapshot("/Users/oscar/Projects/crew")
      : null,
  );

  const result = await readProjectLocalRepoSnapshot(
    {
      cloneUrls: [
        "https://relay.example/git/owner/nuncio-crew",
        "https://github.com/Nuncio-hq/crew.git",
      ],
      defaultBranch: "main",
      localWorkspacePath: null,
      projectDtag: "nuncio-crew",
      reposDir: "/Users/oscar/Projects",
    },
    harness.dependencies,
  );

  assert.equal(result?.path, "/Users/oscar/Projects/crew");
  assert.deepEqual(
    harness.calls.map((call) => call.cloneUrl),
    [
      "https://relay.example/git/owner/nuncio-crew",
      "https://github.com/Nuncio-hq/crew.git",
    ],
  );
});

test("the first resolving clone URL stops the search", async () => {
  const harness = dependencies();

  const result = await readProjectLocalRepoSnapshot(
    {
      cloneUrls: [
        "https://relay.example/git/owner/nuncio-crew",
        "https://github.com/Nuncio-hq/crew.git",
      ],
      defaultBranch: "main",
      projectDtag: "nuncio-crew",
      reposDir: "/Users/oscar/Projects",
    },
    harness.dependencies,
  );

  assert.equal(result?.path, LOCAL_PATH);
  assert.equal(harness.calls.length, 1);
});

test("a selected tag declines the local read instead of serving branch data", async () => {
  const harness = dependencies();

  assert.equal(
    await readProjectLocalRepoSnapshot(
      {
        cloneUrl: "https://relay.example/git/owner/nuncio-crew",
        defaultBranch: "main",
        projectDtag: "nuncio-crew",
        reposDir: "/Users/oscar/Projects",
        selectedTag: "v0.5.18",
      },
      harness.dependencies,
    ),
    null,
  );
  assert.equal(harness.calls.length, 0);
});

test("a selected tag also declines the exact linked-workspace read", async () => {
  const harness = dependencies();

  assert.equal(
    await readProjectLocalRepoSnapshot(
      {
        defaultBranch: "main",
        localWorkspacePath: LOCAL_PATH,
        localWorkspaceStatus: "linked",
        projectDtag: "nuncio-crew",
        selectedTag: "v0.5.18",
      },
      harness.dependencies,
    ),
    null,
  );
  assert.equal(harness.calls.length, 0);
});

test("the source label is honest while a tag is selected", () => {
  assert.deepEqual(
    localWorkspaceSourceState({
      hasSnapshot: true,
      isError: false,
      isLinked: true,
      isLoading: false,
      isTagSelected: true,
    }),
    { disabled: true, label: "Local unavailable" },
  );
  // A pending read still reports checking, not unavailable.
  assert.deepEqual(
    localWorkspaceSourceState({
      hasSnapshot: false,
      isError: false,
      isLinked: true,
      isLoading: true,
      isTagSelected: false,
    }),
    { disabled: false, label: "Local checking" },
  );
});

test("local checkout matching considers every advertised clone URL", () => {
  const multiUrl = repository({
    cloneUrls: [
      "https://relay.example/git/owner/nuncio-crew",
      "https://github.com/Nuncio-hq/crew.git",
    ],
    dtag: "nuncio-crew",
  });

  assert.equal(isProjectLocal(multiUrl, new Set(["crew"])), true);
  assert.equal(isProjectLocal(multiUrl, new Set(["unrelated"])), false);
});

test("the local snapshot query passes the whole clone-URL list and the selected tag", () => {
  const source = readFileSync(new URL("./hooks.ts", import.meta.url), "utf8");
  assert.ok(
    source.includes("cloneUrls: cloneUrlList(project)"),
    "the local snapshot read should receive every advertised clone URL",
  );
  assert.ok(
    source.includes("selectedTag"),
    "the local snapshot query should forward the selected tag",
  );
});
