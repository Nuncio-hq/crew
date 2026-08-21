import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  createProjectThreadPeekFeedSelector,
  deriveProjectThreadPhaseStates,
  formatProjectThreadPeekText,
  mapProjectThreadPeekFeedItems,
  mergeProjectThreadPeekEvents,
  previewProjectThreadPeekText,
  resolveProjectThreadPeekMode,
} from "./projectThreadMissionControl.ts";

const readyWorkspace = {
  status: "ready",
  agentPubkey: "agent",
  baseSource: "remote",
  baseRevision: "abc",
  branch: "feat/issue-82",
  conversationId: "conversation-82",
  rootEventId: "a".repeat(64),
  repositoryPath: "/tmp/crew",
  remoteDefaultBranch: "main",
  commitsBehindRemote: 0,
  worktreeName: "crew-82",
  worktreePath: "/tmp/crew-82",
};

const mergedPullRequest = {
  checks: [{ name: "Crew CI", state: "SUCCESS", url: null, workflow: null }],
  isDraft: false,
  state: "MERGED",
};
const draftPullRequest = {
  checks: [],
  isDraft: true,
  state: "OPEN",
};
const failingPullRequest = {
  checks: [{ name: "Crew CI", state: "FAILURE", url: null, workflow: null }],
  isDraft: false,
  state: "OPEN",
};

describe("project thread phase derivation", () => {
  it("marks a failed workspace red while the thread task is complete", () => {
    const phases = deriveProjectThreadPhaseStates({
      hasThread: true,
      pullRequest: null,
      steps: [{ pubkey: "agent", source: "root", status: "working" }],
      workspace: {
        status: "error",
        agentPubkey: "agent",
        conversationId: "conversation-82",
        message: "worktree setup failed",
        rootEventId: "a".repeat(64),
      },
    });

    assert.equal(phases.task, "complete");
    assert.equal(phases.workspace, "failed");
    assert.equal(phases.handoff, "active");
  });

  it("marks a merged PR and passing CI complete", () => {
    const phases = deriveProjectThreadPhaseStates({
      hasThread: true,
      pullRequest: mergedPullRequest,
      steps: [{ pubkey: "agent", source: "root", status: "done" }],
      workspace: readyWorkspace,
    });

    assert.equal(phases.pr, "complete");
    assert.equal(phases.ci, "complete");
  });

  it("keeps PR and CI pending when no PR is linked", () => {
    const phases = deriveProjectThreadPhaseStates({
      hasThread: true,
      pullRequest: null,
      steps: [],
      workspace: { status: "pending" },
    });

    assert.equal(phases.pr, "pending");
    assert.equal(phases.ci, "pending");
  });

  it("marks waiting-on-user when the handoff flag is set", () => {
    const phases = deriveProjectThreadPhaseStates({
      hasThread: true,
      pullRequest: null,
      steps: [{ pubkey: "agent", source: "root", status: "working" }],
      waitingOnUser: true,
      workspace: readyWorkspace,
    });

    assert.equal(phases.handoff, "waiting-on-user");
  });

  it("marks a mid-handoff with done and queued steps active", () => {
    const phases = deriveProjectThreadPhaseStates({
      hasThread: true,
      pullRequest: null,
      steps: [
        { pubkey: "done", source: "root", status: "done" },
        { pubkey: "queued", source: "reply", status: "queued" },
      ],
      workspace: readyWorkspace,
    });

    assert.equal(phases.handoff, "active");
  });

  it("keeps a draft/open PR active", () => {
    const phases = deriveProjectThreadPhaseStates({
      hasThread: true,
      pullRequest: draftPullRequest,
      steps: [],
      workspace: readyWorkspace,
    });

    assert.equal(phases.pr, "active");
  });

  it("marks CI failure failed", () => {
    const phases = deriveProjectThreadPhaseStates({
      hasThread: true,
      pullRequest: failingPullRequest,
      steps: [],
      workspace: readyWorkspace,
    });

    assert.equal(phases.ci, "failed");
  });
});

const thought = {
  id: "thought-1",
  type: "thought",
  renderClass: "thought",
  title: "Thinking",
  text: "Trace the workspace projection first.",
  timestamp: "2026-08-07T10:00:00.000Z",
};
const tool = {
  id: "tool-1",
  type: "tool",
  renderClass: "shell",
  descriptor: {
    renderClass: "shell",
    label: "Run tests",
    preview: "pnpm test",
  },
  title: "Run tests",
  toolName: "terminal",
  buzzToolName: null,
  status: "completed",
  args: { command: "pnpm test" },
  result: "127 tests passed",
  isError: false,
  timestamp: "2026-08-07T10:00:01.000Z",
  startedAt: "2026-08-07T10:00:01.000Z",
  completedAt: "2026-08-07T10:00:02.000Z",
};

describe("project thread transcript peek", () => {
  it("maps thinking separately from tool calls and their result line", () => {
    const feed = mapProjectThreadPeekFeedItems([thought, tool]);

    assert.deepEqual(feed[0], {
      id: "thought-1",
      kind: "thinking",
      text: "Trace the workspace projection first.",
    });
    assert.equal(feed[1].kind, "tool");
    assert.equal(feed[1].headline, "Run tests · pnpm test");
    assert.equal(feed[1].result, "127 tests passed");
    assert.equal(feed[1].failed, false);
    assert.equal(feed[1].status, "done");
  });

  it("marks executing tools as running in the peek feed", () => {
    const feed = mapProjectThreadPeekFeedItems([
      {
        ...tool,
        id: "tool-running",
        status: "executing",
        result: "",
        completedAt: null,
      },
    ]);

    assert.equal(feed[0].kind, "tool");
    assert.equal(feed[0].status, "running");
    assert.equal(feed[0].result, null);
  });

  it("humanizes escaped one-line dumps for peek previews", () => {
    const escaped =
      "3-line story:\\n- assigned work\\n- current status\\n- what was omitted";
    const formatted = formatProjectThreadPeekText(escaped);
    assert.ok(formatted.includes("\n"));
    assert.equal(formatted.includes("\\n"), false);

    const preview = previewProjectThreadPeekText(escaped);
    assert.equal(preview.truncated, true);
    assert.equal(preview.preview, "3-line story:");
  });

  it("sources history mode from archived events scoped to the conversation", () => {
    const archived = [
      {
        seq: 1,
        timestamp: "2026-08-07T09:00:00.000Z",
        conversationId: "other",
      },
      {
        seq: 2,
        timestamp: "2026-08-07T10:00:00.000Z",
        conversationId: "conversation-82",
      },
    ];

    const selected = mergeProjectThreadPeekEvents(
      [],
      archived,
      "conversation-82",
    );

    assert.equal(
      resolveProjectThreadPeekMode(false, selected.length),
      "history",
    );
    assert.deepEqual(
      selected.map((event) => event.seq),
      [2],
    );
  });

  it("keeps the feed reference stable for an unchanged transcript", () => {
    const selectFeed = createProjectThreadPeekFeedSelector();
    const transcript = [thought, tool];

    const first = selectFeed(transcript);
    const second = selectFeed([...transcript]);

    assert.strictEqual(second, first);
  });
});
