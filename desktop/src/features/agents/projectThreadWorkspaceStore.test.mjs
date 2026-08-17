import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  PROJECT_THREAD_WORKSPACE_ROOT_CAP,
  clearSavedProjectThreadWorkspaceSnapshot,
  getProjectThreadWorkspaceSnapshot,
  ingestProjectThreadWorkspaceEvent,
  resetProjectThreadWorkspaceStore,
  restoreProjectThreadWorkspacesForCommunity,
  saveProjectThreadWorkspacesForCommunity,
} from "./projectThreadWorkspaceStore.ts";

const root = "a".repeat(64);

function event(
  kind,
  payload,
  conversationId = "conversation-a",
  { agentIndex = 0, seq = 1, timestamp = "2026-07-31T00:00:00.000Z" } = {},
) {
  return {
    seq,
    timestamp,
    kind,
    agentIndex,
    channelId: "channel-a",
    conversationId,
    sessionId: null,
    turnId: "turn-a",
    payload,
  };
}

beforeEach(() => {
  resetProjectThreadWorkspaceStore();
  clearSavedProjectThreadWorkspaceSnapshot("community-a");
  clearSavedProjectThreadWorkspaceSnapshot("community-b");
});

function readyPayload(rootEventId, branch = "buzz/aaaaaaaaaaaa") {
  return {
    rootEventId,
    branch,
    worktreePath: `/tmp/.buzz-worktrees/${branch.slice(5)}`,
    worktreeName: branch.slice(5),
    baseRevision: "deadbeef",
    baseSource: "remote",
    remoteDefaultBranch: "main",
    commitsBehindRemote: 0,
    repositoryPath: "/tmp/project",
  };
}

test("workspace stays pending until a valid lifecycle frame arrives", () => {
  assert.deepEqual(getProjectThreadWorkspaceSnapshot(root), {
    status: "pending",
  });
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event("thread_workspace_ready", { rootEventId: "bad" }),
  );
  assert.equal(getProjectThreadWorkspaceSnapshot(root).status, "pending");
});

test("ready projection preserves verified worktree identity", () => {
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event("thread_workspace_ready", {
      rootEventId: root,
      branch: "buzz/aaaaaaaaaaaa",
      worktreePath: "/tmp/.buzz-worktrees/app-aaaaaaaaaaaa",
      worktreeName: "app-aaaaaaaaaaaa",
      baseRevision: "deadbeef",
    }),
  );

  assert.deepEqual(getProjectThreadWorkspaceSnapshot(root), {
    status: "ready",
    agentPubkey: "agent-a",
    baseSource: "local-fallback",
    baseRevision: "deadbeef",
    branch: "buzz/aaaaaaaaaaaa",
    conversationId: "conversation-a",
    rootEventId: root,
    remoteDefaultBranch: null,
    commitsBehindRemote: null,
    repositoryPath: null,
    worktreeName: "app-aaaaaaaaaaaa",
    worktreePath: "/tmp/.buzz-worktrees/app-aaaaaaaaaaaa",
  });
});

test("error projection is scoped to its exact root", () => {
  const otherRoot = "b".repeat(64);
  ingestProjectThreadWorkspaceEvent(
    "agent-b",
    event("thread_workspace_error", {
      rootEventId: otherRoot,
      message: "branch already checked out",
    }),
  );

  assert.equal(getProjectThreadWorkspaceSnapshot(root).status, "pending");
  assert.deepEqual(getProjectThreadWorkspaceSnapshot(otherRoot), {
    status: "error",
    agentPubkey: "agent-b",
    conversationId: "conversation-a",
    message: "branch already checked out",
    rootEventId: otherRoot,
  });
});

test("missing-folder error preserves recover reason without leaking a path", () => {
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event("thread_workspace_error", {
      rootEventId: root,
      message: "The Project folder is gone. Pick a workspace again.",
      reason: "missing-folder",
    }),
  );

  assert.deepEqual(getProjectThreadWorkspaceSnapshot(root), {
    status: "error",
    agentPubkey: "agent-a",
    conversationId: "conversation-a",
    message: "The Project folder is gone. Pick a workspace again.",
    reason: "missing-folder",
    rootEventId: root,
  });
});

test("newer ready projection rejects a stale error frame", () => {
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event("thread_workspace_ready", readyPayload(root), "conversation-a", {
      seq: 5,
      timestamp: "2026-07-31T00:00:05.000Z",
    }),
  );
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event(
      "thread_workspace_error",
      { rootEventId: root, message: "stale failure" },
      "conversation-a",
      { seq: 4, timestamp: "2026-07-31T00:00:04.000Z" },
    ),
  );

  assert.equal(getProjectThreadWorkspaceSnapshot(root).status, "ready");
});

test("newer error projection rejects a stale ready frame", () => {
  ingestProjectThreadWorkspaceEvent(
    "agent-b",
    event(
      "thread_workspace_error",
      { rootEventId: root, message: "authoritative failure" },
      "conversation-a",
      { seq: 8, timestamp: "2026-07-31T00:00:08.000Z" },
    ),
  );
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event("thread_workspace_ready", readyPayload(root), "conversation-a", {
      seq: 7,
      timestamp: "2026-07-31T00:00:07.000Z",
    }),
  );

  const snapshot = getProjectThreadWorkspaceSnapshot(root);
  assert.equal(snapshot.status, "error");
  assert.equal(snapshot.message, "authoritative failure");
});

test("equal timestamp and seq use agent identity as deterministic watermark", () => {
  ingestProjectThreadWorkspaceEvent(
    "agent-b",
    event(
      "thread_workspace_error",
      { rootEventId: root, message: "agent-b wins" },
      "conversation-a",
      { seq: 4 },
    ),
  );
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event("thread_workspace_ready", readyPayload(root), "conversation-a", {
      seq: 4,
    }),
  );

  const snapshot = getProjectThreadWorkspaceSnapshot(root);
  assert.equal(snapshot.status, "error");
  assert.equal(snapshot.message, "agent-b wins");
});

test("community round-trip restores its projection without cross-community leak", () => {
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event("thread_workspace_ready", readyPayload(root, "buzz/community-a")),
  );
  saveProjectThreadWorkspacesForCommunity("community-a");
  resetProjectThreadWorkspaceStore();
  assert.equal(
    getProjectThreadWorkspaceSnapshot(root).status,
    "pending",
    "saved community A state must remain inaccessible while community B is active",
  );

  ingestProjectThreadWorkspaceEvent(
    "agent-b",
    event("thread_workspace_error", {
      rootEventId: root,
      message: "community-b failure",
    }),
  );
  saveProjectThreadWorkspacesForCommunity("community-b");
  resetProjectThreadWorkspaceStore();

  restoreProjectThreadWorkspacesForCommunity("community-a");
  const communityA = getProjectThreadWorkspaceSnapshot(root);
  assert.equal(communityA.status, "ready");
  assert.equal(communityA.branch, "buzz/community-a");

  saveProjectThreadWorkspacesForCommunity("community-a");
  resetProjectThreadWorkspaceStore();
  restoreProjectThreadWorkspacesForCommunity("community-b");
  const communityB = getProjectThreadWorkspaceSnapshot(root);
  assert.equal(communityB.status, "error");
  assert.equal(communityB.message, "community-b failure");
});

test("root projection evicts the least-recently-used entry at its cap", () => {
  const roots = Array.from(
    { length: PROJECT_THREAD_WORKSPACE_ROOT_CAP },
    (_, i) => i.toString(16).padStart(64, "0"),
  );
  for (const [index, rootEventId] of roots.entries()) {
    ingestProjectThreadWorkspaceEvent(
      "agent-a",
      event(
        "thread_workspace_ready",
        readyPayload(rootEventId),
        "conversation",
        {
          seq: index,
          timestamp: new Date(Date.UTC(2026, 6, 31, 0, 0, index)).toISOString(),
        },
      ),
    );
  }

  // Refresh the first root, making the second root the least recently used.
  assert.equal(getProjectThreadWorkspaceSnapshot(roots[0]).status, "ready");
  const overflowRoot = "f".repeat(64);
  ingestProjectThreadWorkspaceEvent(
    "agent-a",
    event(
      "thread_workspace_ready",
      readyPayload(overflowRoot),
      "conversation",
      {
        seq: PROJECT_THREAD_WORKSPACE_ROOT_CAP,
        timestamp: "2026-07-31T01:00:00.000Z",
      },
    ),
  );

  assert.equal(getProjectThreadWorkspaceSnapshot(roots[0]).status, "ready");
  assert.equal(getProjectThreadWorkspaceSnapshot(roots[1]).status, "pending");
  assert.equal(getProjectThreadWorkspaceSnapshot(overflowRoot).status, "ready");
});
