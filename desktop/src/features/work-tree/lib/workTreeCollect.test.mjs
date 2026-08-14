import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { collectWorkThreads } from "./workTreeCollect.ts";

const ROOT = "1".repeat(64);
const CHANNEL = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const NOW = 1_700_000_000_000;

describe("collectWorkThreads", () => {
  it("drops talk-only threads and keeps workspace or needs-you", () => {
    const rows = collectWorkThreads({
      channelNameById: new Map([[CHANNEL, "engineering"]]),
      now: NOW,
      registry: [],
      sessions: [],
      needsYou: [
        {
          channelId: CHANNEL,
          conversationId: "need",
          createdAt: NOW - 5_000,
          rootEventId: ROOT,
        },
      ],
      titlesByRoot: new Map([[ROOT, "Fix paywall crash"]]),
      workspaces: [],
    });
    assert.equal(rows.length, 1);
    assert.equal(rows[0].title, "Fix paywall crash");
    assert.equal(rows[0].status, "needs-you");
    assert.equal(rows[0].hasWorkspaceBinding, false);
  });

  it("marks workspace rows and joins a recent working session", () => {
    const rows = collectWorkThreads({
      channelNameById: new Map([[CHANNEL, "engineering"]]),
      now: NOW,
      registry: [
        {
          branch: "fix-paywall",
          checks: "passing",
          lastUsedAt: Math.floor(NOW / 1000) - 60,
          prDraft: false,
          prNumber: 42,
          prState: "OPEN",
          repositoryPath: "/tmp/crew",
          rootEventId: ROOT,
        },
      ],
      sessions: [
        {
          channelId: CHANNEL,
          conversationId: "ws",
          lastSeenAt: NOW - 12_000,
          rootEventId: ROOT,
          sleeping: false,
          title: null,
          working: true,
        },
      ],
      needsYou: [],
      titlesByRoot: new Map([[ROOT, "Fix paywall crash"]]),
      workspaces: [
        {
          branch: "fix-paywall",
          channelId: CHANNEL,
          conversationId: "ws",
          lastActivityAt: NOW - 12_000,
          repositoryPath: "/tmp/crew",
          rootEventId: ROOT,
        },
      ],
    });
    assert.equal(rows.length, 1);
    assert.equal(rows[0].status, "working");
    assert.equal(rows[0].branch, "fix-paywall");
    assert.equal(rows[0].prNumber, 42);
    assert.equal(rows[0].ciGlyph, "pass");
    assert.equal(rows[0].hasWorkspaceBinding, true);
  });

  it("drops a stale session with no workspace and no needs-you", () => {
    const rows = collectWorkThreads({
      channelNameById: new Map([[CHANNEL, "engineering"]]),
      now: NOW,
      registry: [],
      sessions: [
        {
          channelId: CHANNEL,
          conversationId: "talk",
          lastSeenAt: NOW - 49 * 60 * 60 * 1_000,
          rootEventId: ROOT,
          sleeping: false,
          title: "Just chatting",
          working: false,
        },
      ],
      needsYou: [],
      titlesByRoot: new Map([[ROOT, "Just chatting"]]),
      workspaces: [],
    });
    assert.equal(rows.length, 0);
  });

  it("joins a workspace onto a needs-you row that already has the channel", () => {
    const rows = collectWorkThreads({
      channelNameById: new Map([[CHANNEL, "engineering"]]),
      now: NOW,
      registry: [],
      sessions: [],
      needsYou: [
        {
          channelId: CHANNEL,
          conversationId: "need",
          createdAt: NOW - 5_000,
          rootEventId: ROOT,
        },
      ],
      titlesByRoot: new Map([[ROOT, "Fix paywall crash"]]),
      workspaces: [
        {
          branch: "fix-paywall",
          channelId: null,
          conversationId: null,
          lastActivityAt: NOW,
          repositoryPath: "/tmp/crew",
          rootEventId: ROOT,
        },
      ],
    });
    assert.equal(rows.length, 1);
    assert.equal(rows[0].hasWorkspaceBinding, true);
    assert.equal(rows[0].branch, "fix-paywall");
    assert.equal(rows[0].status, "needs-you");
  });

  it("stays cheap over hundreds of eligible threads", () => {
    const workspaces = Array.from({ length: 400 }, (_, index) => ({
      branch: `b${index}`,
      channelId: CHANNEL,
      conversationId: `c${index}`,
      lastActivityAt: NOW - index * 1_000,
      repositoryPath: "/tmp/crew",
      rootEventId: String(index).padStart(64, "0"),
    }));
    const started = performance.now();
    const rows = collectWorkThreads({
      channelNameById: new Map([[CHANNEL, "engineering"]]),
      now: NOW,
      registry: [],
      sessions: [],
      needsYou: [],
      workspaces,
    });
    const elapsed = performance.now() - started;
    assert.equal(rows.length, 400);
    assert.ok(elapsed < 50, `collect took ${elapsed}ms`);
  });
});
