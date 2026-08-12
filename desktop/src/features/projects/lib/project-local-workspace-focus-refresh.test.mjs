import assert from "node:assert/strict";
import test from "node:test";

import {
  LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS,
  LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS,
  createLocalWorkspaceSnapshotFocusRefresh,
  isLocalRepoSnapshotQueryKey,
  shouldScheduleLocalWorkspaceSnapshotFocusRefresh,
} from "./project-local-workspace-focus-refresh.ts";
import {
  readExactLocalWorkspaceSnapshot,
  readProjectLocalRepoSnapshot,
} from "./project-exact-local-workspace.ts";

// Deterministic timer host — same style as trailingDebounce.test.mjs.
function makeHost() {
  let nextId = 1;
  const pending = new Map();
  return {
    host: {
      setTimeout: (handler, ms) => {
        const id = nextId++;
        pending.set(id, { handler, remaining: ms });
        return id;
      },
      clearTimeout: (id) => {
        pending.delete(id);
      },
    },
    advance: (ms) => {
      for (const [id, t] of [...pending]) {
        t.remaining -= ms;
        if (t.remaining <= 0) {
          pending.delete(id);
          t.handler();
        }
      }
    },
  };
}

const LOCAL_PATH = "/Users/oscar/Projects/Nùncio Crew";

function exactReaderDependencies(overrides = {}) {
  const calls = [];
  return {
    calls,
    dependencies: {
      basename: async () => "Nùncio Crew",
      dirname: async () => "/Users/oscar/Projects",
      getLocalSnapshot: async (input) => {
        calls.push(input);
        return {
          path: LOCAL_PATH,
          snapshot: {
            commits: [],
            contributors: [],
            files: [],
            latestCommit: null,
          },
        };
      },
      normalize: async (path) => path,
      ...overrides,
    },
  };
}

test("isLocalRepoSnapshotQueryKey matches Project local snapshot keys only", () => {
  assert.equal(
    isLocalRepoSnapshotQueryKey([
      "project",
      "abc",
      "local-repo-snapshot",
      "/path",
      "linked",
      "default",
      "main",
    ]),
    true,
  );
  assert.equal(
    isLocalRepoSnapshotQueryKey(["project", "abc", "repo-snapshot"]),
    false,
  );
  assert.equal(
    isLocalRepoSnapshotQueryKey(["projects", "repo-snapshots"]),
    false,
  );
  assert.equal(isLocalRepoSnapshotQueryKey(["channels"]), false);
});

test("focus schedules a refresh after the trailing debounce quiet window", () => {
  const { host, advance } = makeHost();
  let refreshCalls = 0;
  const now = 10_000;
  const controller = createLocalWorkspaceSnapshotFocusRefresh(
    () => {
      refreshCalls += 1;
    },
    { host, now: () => now },
  );

  controller.onAppFocus();
  assert.equal(refreshCalls, 0, "must not refresh synchronously on focus");
  advance(LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS - 1);
  assert.equal(refreshCalls, 0, "still waiting for quiet window");
  advance(1);
  assert.equal(refreshCalls, 1, "refresh happens once after debounce");
});

test("rapid focus/blur coalesces into a single refresh (debounce bites)", () => {
  const { host, advance } = makeHost();
  let refreshCalls = 0;
  const now = 10_000;
  const controller = createLocalWorkspaceSnapshotFocusRefresh(
    () => {
      refreshCalls += 1;
    },
    { host, now: () => now },
  );

  // Burst of focus signals — trailing debounce must collapse them.
  controller.onAppFocus();
  advance(100);
  controller.onAppFocus();
  advance(100);
  controller.onAppFocus();
  assert.equal(refreshCalls, 0, "must not fire during the focus burst");

  advance(LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS);
  assert.equal(refreshCalls, 1, "exactly one refresh after the burst settles");

  // Prove the debounce assertion is load-bearing: without restarting the
  // quiet window, three spaced triggers would schedule three refreshes.
  // (A broken non-debounced implementation that called refresh() inside
  // onAppFocus would already have failed the asserts above.)
});

test("min interval suppresses a second refresh after a recent re-read", () => {
  const { host, advance } = makeHost();
  let refreshCalls = 0;
  let now = 10_000;
  const controller = createLocalWorkspaceSnapshotFocusRefresh(
    () => {
      refreshCalls += 1;
    },
    { host, now: () => now },
  );

  controller.onAppFocus();
  advance(LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS);
  assert.equal(refreshCalls, 1);

  // Another focus shortly after the refresh — still within min interval.
  now = 10_000 + LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS - 1;
  controller.onAppFocus();
  advance(LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS);
  assert.equal(refreshCalls, 1, "min interval blocks thrash");

  now = 10_000 + LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS;
  controller.onAppFocus();
  advance(LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS);
  assert.equal(refreshCalls, 2, "allowed again after min interval");
});

test("shouldScheduleLocalWorkspaceSnapshotFocusRefresh rate-limits", () => {
  assert.equal(
    shouldScheduleLocalWorkspaceSnapshotFocusRefresh({
      lastRefreshAt: 1_000,
      now: 1_000 + LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS - 1,
    }),
    false,
  );
  assert.equal(
    shouldScheduleLocalWorkspaceSnapshotFocusRefresh({
      lastRefreshAt: 1_000,
      now: 1_000 + LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS,
    }),
    true,
  );
});

test("focus refresh path still enforces exact-path containment (no fallback)", async () => {
  // The focus controller only schedules a re-read; the read still goes through
  // readExactLocalWorkspaceSnapshot. Prove that path is not weakened: linked
  // workspaces call the native reader with folder basename (not Project d-tag)
  // and cloneUrl null, and a mismatched resolved path fails closed.
  const harness = exactReaderDependencies();

  await readProjectLocalRepoSnapshot(
    {
      cloneUrl: "https://relay.example/git/owner/other",
      defaultBranch: "main",
      localWorkspacePath: LOCAL_PATH,
      localWorkspaceStatus: "linked",
      projectDtag: "renamed-project",
      reposDir: "/Users/oscar/BuzzRepos",
    },
    harness.dependencies,
  );

  assert.deepEqual(harness.calls, [
    {
      baseBranch: null,
      cloneUrl: null,
      defaultBranch: "main",
      projectDtag: "Nùncio Crew",
      reposDir: "/Users/oscar/Projects",
    },
  ]);

  const mismatch = exactReaderDependencies({
    getLocalSnapshot: async () => ({
      path: "/Users/oscar/Projects/another-crew",
      snapshot: {
        commits: [],
        contributors: [],
        files: [],
        latestCommit: null,
      },
    }),
  });

  await assert.rejects(
    readExactLocalWorkspaceSnapshot(
      {
        defaultBranch: "main",
        localWorkspacePath: LOCAL_PATH,
        projectDtag: "crew",
      },
      mismatch.dependencies,
    ),
    /different folder/,
  );
});

test("cancel drops a pending focus refresh", () => {
  const { host, advance } = makeHost();
  let refreshCalls = 0;
  const controller = createLocalWorkspaceSnapshotFocusRefresh(
    () => {
      refreshCalls += 1;
    },
    { host, now: () => 10_000 },
  );

  controller.onAppFocus();
  controller.cancel();
  advance(LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS);
  assert.equal(refreshCalls, 0);
});
