import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  applyCollapsedArrival,
  buildWorkTreeFolder,
  capThreads,
  disclosureAfterLiveArrival,
  folderBadge,
  isRecentSession,
  isWorkThreadEligible,
  projectFolderChannelIds,
  shouldAutoCollapse,
  sortWorkThreads,
  workThreadStatus,
} from "./workTreeEligibility.ts";
import { WORK_TREE_QUIET_MS } from "./workTreeTypes.ts";

const NOW = 1_000_000_000_000;
const ROOT_A = "a".repeat(64);
const ROOT_B = "b".repeat(64);
const ROOT_C = "c".repeat(64);
const CHANNEL = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";

function thread(overrides = {}) {
  return {
    branch: "feat-x",
    channelId: CHANNEL,
    channelName: "glowmax",
    ciGlyph: "pass",
    conversationId: "c1",
    hasWorkspaceBinding: true,
    lastActivityAt: NOW - 60_000,
    prNumber: 42,
    prTone: "open",
    status: "working",
    threadRootId: ROOT_A,
    title: "Fix paywall crash",
    unread: false,
    ...overrides,
  };
}

describe("isWorkThreadEligible", () => {
  it("is true for workspace, recent session, or needs-you", () => {
    assert.equal(
      isWorkThreadEligible({
        hasWorkspaceBinding: true,
        hasRecentSession: false,
        hasNeedsYou: false,
      }),
      true,
    );
    assert.equal(
      isWorkThreadEligible({
        hasWorkspaceBinding: false,
        hasRecentSession: true,
        hasNeedsYou: false,
      }),
      true,
    );
    assert.equal(
      isWorkThreadEligible({
        hasWorkspaceBinding: false,
        hasRecentSession: false,
        hasNeedsYou: true,
      }),
      true,
    );
  });

  it("is false for pure conversation (no binding, session, or needs-you)", () => {
    assert.equal(
      isWorkThreadEligible({
        hasWorkspaceBinding: false,
        hasRecentSession: false,
        hasNeedsYou: false,
      }),
      false,
    );
  });
});

describe("isRecentSession", () => {
  it("treats lastSeen within 48h as recent", () => {
    assert.equal(isRecentSession(NOW - WORK_TREE_QUIET_MS + 1, NOW), true);
    assert.equal(isRecentSession(NOW - WORK_TREE_QUIET_MS - 1, NOW), false);
  });
});

describe("workThreadStatus", () => {
  it("gives needs-you precedence over working", () => {
    assert.equal(
      workThreadStatus({
        hasNeedsYou: true,
        isWorking: true,
        isSleeping: false,
      }),
      "needs-you",
    );
    assert.equal(
      workThreadStatus({
        hasNeedsYou: false,
        isWorking: true,
        isSleeping: false,
      }),
      "working",
    );
    assert.equal(
      workThreadStatus({
        hasNeedsYou: false,
        isWorking: true,
        isSleeping: true,
      }),
      "sleeping",
    );
  });
});

describe("folderBadge", () => {
  it("prefers needs-you over a live working count", () => {
    assert.deepEqual(
      folderBadge([
        thread({ status: "working" }),
        thread({ threadRootId: ROOT_B, status: "needs-you" }),
      ]),
      { kind: "needs-you" },
    );
  });

  it("omits zero-count badges", () => {
    assert.equal(folderBadge([thread({ status: "sleeping" })]), null);
    assert.equal(folderBadge([]), null);
  });

  it("counts live working threads", () => {
    assert.deepEqual(
      folderBadge([
        thread({ status: "working" }),
        thread({ threadRootId: ROOT_B, status: "working" }),
        thread({ threadRootId: ROOT_C, status: "sleeping" }),
      ]),
      { kind: "live", count: 2 },
    );
  });
});

describe("cap + more", () => {
  it("caps at 5 and reports hidden count", () => {
    const threads = Array.from({ length: 8 }, (_, index) =>
      thread({
        lastActivityAt: NOW - index * 1_000,
        threadRootId: String(index).padStart(64, "0"),
        title: `T${index}`,
      }),
    );
    const capped = capThreads(sortWorkThreads(threads), 5, false);
    assert.equal(capped.visible.length, 5);
    assert.equal(capped.hiddenCount, 3);
    assert.equal(capped.visible[0].title, "T0");
    const expanded = capThreads(sortWorkThreads(threads), 5, true);
    assert.equal(expanded.visible.length, 8);
    assert.equal(expanded.hiddenCount, 0);
  });
});

describe("auto-collapse", () => {
  it("collapses after 48h unless pinned", () => {
    assert.equal(
      shouldAutoCollapse({
        lastActivityAt: NOW - WORK_TREE_QUIET_MS - 1,
        now: NOW,
        pinned: false,
      }),
      true,
    );
    assert.equal(
      shouldAutoCollapse({
        lastActivityAt: NOW - WORK_TREE_QUIET_MS - 1,
        now: NOW,
        pinned: true,
      }),
      false,
    );
  });

  it("does not auto-expand a collapsed folder when a row arrives", () => {
    const after = disclosureAfterLiveArrival({
      expanded: false,
      pinned: false,
    });
    assert.equal(after.expanded, false);
    const arrival = applyCollapsedArrival({
      disclosure: after,
      lastActivityAt: NOW,
      now: NOW,
    });
    assert.equal(arrival.expanded, false);
  });
});

describe("buildWorkTreeFolder", () => {
  it("renders folder-only when there are no work threads", () => {
    const folder = buildWorkTreeFolder({
      channelId: CHANNEL,
      channelName: "glowmax",
      now: NOW,
      threads: [],
      timelineUnread: false,
    });
    assert.equal(folder.visibleThreads.length, 0);
    assert.equal(folder.hiddenCount, 0);
    assert.equal(folder.badge, null);
    assert.equal(folder.threads.length, 0);
  });
});

describe("projectFolderChannelIds", () => {
  it("keeps shared access channels out of the folder set", () => {
    const ids = projectFolderChannelIds([
      {
        projectChannelId: null,
        repositories: [
          { channelId: "general", repoAddress: "30617:aa:buzz" },
          { channelId: "general", repoAddress: "30617:bb:tools" },
        ],
      },
    ]);
    assert.equal(ids.has("general"), false);
  });

  it("promotes an exclusive repo binding and an explicit project channel", () => {
    const ids = projectFolderChannelIds([
      {
        projectChannelId: "glowmax-id",
        repositories: [
          { channelId: "engineering-id", repoAddress: "30617:aa:glowmax" },
        ],
      },
    ]);
    assert.equal(ids.has("glowmax-id"), true);
    assert.equal(ids.has("engineering-id"), true);
  });
});
