import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  __setProjectWorktreeRegistryForTests,
  getProjectWorktreeEntryByRoot,
  invalidateProjectWorktreeRegistry,
  resetProjectWorktreeRegistryStore,
} from "./projectWorktreeRegistryStore.ts";

const repo = "/tmp/crew";
const root = "a".repeat(64);

function registry(entries) {
  return {
    repositoryPath: repo,
    managedRoot: "/tmp/.buzz-worktrees",
    github: "available",
    entries,
  };
}

beforeEach(() => {
  resetProjectWorktreeRegistryStore();
});

test("selector returns null for unknown roots", () => {
  __setProjectWorktreeRegistryForTests(
    repo,
    registry([
      {
        worktreePath: "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
        worktreeName: "crew-aaaaaaaaaaaa",
        branch: "buzz/aaaaaaaaaaaa",
        head: "deadbeef",
        kind: "managed",
        rootEventId: root,
        prunable: false,
        pullRequests: [],
      },
    ]),
  );
  assert.equal(getProjectWorktreeEntryByRoot(repo, "b".repeat(64)), null);
  assert.equal(getProjectWorktreeEntryByRoot(null, root), null);
});

test("selector finds managed entry by root", () => {
  __setProjectWorktreeRegistryForTests(
    repo,
    registry([
      {
        worktreePath: "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
        worktreeName: "crew-aaaaaaaaaaaa",
        branch: "buzz/aaaaaaaaaaaa",
        head: "deadbeef",
        kind: "managed",
        rootEventId: root,
        prunable: false,
        pullRequests: [{ number: 21, state: "OPEN" }],
      },
    ]),
  );
  const entry = getProjectWorktreeEntryByRoot(repo, root);
  assert.equal(entry?.branch, "buzz/aaaaaaaaaaaa");
  assert.equal(entry?.pullRequests[0]?.number, 21);
});

test("epoch reset clears entries", () => {
  __setProjectWorktreeRegistryForTests(
    repo,
    registry([
      {
        worktreePath: "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
        worktreeName: "crew-aaaaaaaaaaaa",
        branch: "buzz/aaaaaaaaaaaa",
        head: "deadbeef",
        kind: "managed",
        rootEventId: root,
        prunable: false,
        pullRequests: [],
      },
    ]),
  );
  resetProjectWorktreeRegistryStore();
  assert.equal(getProjectWorktreeEntryByRoot(repo, root), null);
});

test("invalidate drops a single repo key", () => {
  __setProjectWorktreeRegistryForTests(
    repo,
    registry([
      {
        worktreePath: "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
        worktreeName: "crew-aaaaaaaaaaaa",
        branch: "buzz/aaaaaaaaaaaa",
        head: "deadbeef",
        kind: "managed",
        rootEventId: root,
        prunable: false,
        pullRequests: [],
      },
    ]),
  );
  invalidateProjectWorktreeRegistry(repo);
  assert.equal(getProjectWorktreeEntryByRoot(repo, root), null);
});
