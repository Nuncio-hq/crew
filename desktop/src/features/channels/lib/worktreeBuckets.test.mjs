import assert from "node:assert/strict";
import test from "node:test";

import {
  IDLE_QUIET_MS,
  aggregateGithubRollup,
  bucketWorktrees,
  channelWorktreesPillLabel,
  countManagedWorktrees,
  countOpenPullRequests,
  githubAvailabilityNotice,
} from "./worktreeBuckets.ts";

const DAY = 24 * 60 * 60 * 1000;

function entry(overrides = {}) {
  return {
    worktreePath: "/repo/.buzz-worktrees/crew-aaaaaaaaaaaa",
    worktreeName: "crew-aaaaaaaaaaaa",
    branch: "buzz/aaaaaaaaaaaa",
    head: "abc",
    kind: "managed",
    rootEventId: "a".repeat(64),
    prunable: false,
    pullRequests: [],
    linkedIssues: [],
    ...overrides,
  };
}

function pr(overrides = {}) {
  return {
    number: 1,
    state: "OPEN",
    isDraft: false,
    reviewDecision: "",
    checks: "pending",
    additions: 1,
    deletions: 0,
    title: "t",
    url: "https://example.com/1",
    ...overrides,
  };
}

test("orphan no root vs other channel vs active", () => {
  const channelRoot = "b".repeat(64);
  const buckets = bucketWorktrees({
    entries: [
      entry({ rootEventId: null, worktreePath: "/o1", worktreeName: "o1" }),
      entry({
        rootEventId: "c".repeat(64),
        worktreePath: "/o2",
        worktreeName: "o2",
      }),
      entry({
        rootEventId: channelRoot,
        worktreePath: "/a1",
        worktreeName: "a1",
        pullRequests: [pr()],
      }),
    ],
    channelRootIds: new Set([channelRoot]),
    activeRootIds: new Set(),
  });
  const byId = Object.fromEntries(buckets.map((b) => [b.id, b.items]));
  assert.equal(byId.orphan.length, 2);
  assert.equal(byId.orphan[0].orphanReason, "unknown");
  assert.equal(byId.orphan[1].orphanReason, "other-channel");
  assert.equal(byId.active.length, 1);
});

test("external and main never actionable", () => {
  const buckets = bucketWorktrees({
    entries: [
      entry({ kind: "main", branch: "main", rootEventId: null }),
      entry({
        kind: "external",
        branch: "docs/x",
        worktreePath: "/repo/.worktrees/x",
        rootEventId: null,
      }),
    ],
    channelRootIds: new Set(),
    activeRootIds: new Set(),
  });
  assert.equal(buckets.length, 1);
  assert.equal(buckets[0].id, "other");
  assert.equal(buckets[0].readonly, true);
});

test("idle threshold boundary at exactly 7 days", () => {
  const now = Date.parse("2026-08-02T00:00:00.000Z");
  const root = "d".repeat(64);
  const path = "/repo/.buzz-worktrees/crew-dddddddddddd";
  const exactlyIdleAt = Math.floor((now - IDLE_QUIET_MS) / 1000);
  const justUnder = exactlyIdleAt + 1;

  const idleBuckets = bucketWorktrees({
    entries: [entry({ rootEventId: root, worktreePath: path })],
    channelRootIds: new Set([root]),
    activeRootIds: new Set(),
    detailsByPath: new Map([
      [
        path,
        {
          worktreePath: path,
          dirty: false,
          ahead: 0,
          behind: 0,
          lastCommitAt: exactlyIdleAt,
          diskBytes: 1,
        },
      ],
    ]),
    nowMs: now,
  });
  assert.equal(idleBuckets.find((b) => b.id === "idle")?.items.length, 1);

  const activeBuckets = bucketWorktrees({
    entries: [entry({ rootEventId: root, worktreePath: path })],
    channelRootIds: new Set([root]),
    activeRootIds: new Set(),
    detailsByPath: new Map([
      [
        path,
        {
          worktreePath: path,
          dirty: false,
          ahead: 0,
          behind: 0,
          lastCommitAt: justUnder,
          diskBytes: 1,
        },
      ],
    ]),
    nowMs: now,
  });
  assert.equal(
    activeBuckets.find((b) => b.id === "idle"),
    undefined,
  );
  assert.equal(activeBuckets.find((b) => b.id === "active")?.items.length, 1);
  assert.ok(IDLE_QUIET_MS === 7 * DAY);
});

test("ready-to-merge requires approval and passing checks", () => {
  const root = "e".repeat(64);
  const approved = entry({
    rootEventId: root,
    pullRequests: [
      pr({ reviewDecision: "APPROVED", checks: "passing", number: 18 }),
    ],
  });
  const missingChecks = entry({
    rootEventId: root,
    worktreePath: "/other",
    pullRequests: [
      pr({ reviewDecision: "APPROVED", checks: "pending", number: 19 }),
    ],
  });
  const buckets = bucketWorktrees({
    entries: [approved, missingChecks],
    channelRootIds: new Set([root]),
    activeRootIds: new Set(),
  });
  assert.equal(buckets.find((b) => b.id === "ready-to-merge")?.items.length, 1);
  assert.equal(buckets.find((b) => b.id === "active")?.items.length, 1);
});

test("counts managed and open PRs", () => {
  const entries = [
    entry({
      pullRequests: [pr({ number: 1 }), pr({ number: 2, isDraft: true })],
    }),
    entry({
      kind: "external",
      pullRequests: [pr({ number: 9 })],
      worktreePath: "/ext",
    }),
    entry({
      worktreePath: "/m2",
      pullRequests: [pr({ number: 1 }), pr({ number: 3, state: "MERGED" })],
    }),
  ];
  assert.equal(countManagedWorktrees(entries), 2);
  assert.equal(countOpenPullRequests(entries), 2);
});

test("channel worktrees pill label distinguishes degraded GitHub from zero PRs", () => {
  assert.equal(channelWorktreesPillLabel(2, 0, "available"), "2 worktrees");
  assert.equal(
    channelWorktreesPillLabel(2, 1, "available"),
    "2 worktrees · 1 PR open",
  );
  assert.equal(
    channelWorktreesPillLabel(3, 2, "available"),
    "3 worktrees · 2 PRs open",
  );
  assert.equal(
    channelWorktreesPillLabel(2, 0, "cli-missing"),
    "2 worktrees · PRs unavailable",
  );
  assert.equal(
    channelWorktreesPillLabel(2, 0, "cli-failed"),
    "2 worktrees · PRs unavailable",
  );
});

test("github availability notice names the cause", () => {
  assert.equal(githubAvailabilityNotice("available"), null);
  assert.match(
    githubAvailabilityNotice("cli-missing") ?? "",
    /GitHub CLI \(gh\) not found/,
  );
  assert.match(
    githubAvailabilityNotice("cli-failed") ?? "",
    /could not read this repo/,
  );
});

test("aggregateGithubRollup mixed states and dedupes", () => {
  const counts = aggregateGithubRollup([
    entry({
      pullRequests: [
        pr({ number: 1, state: "OPEN" }),
        pr({ number: 2, state: "OPEN", isDraft: true }),
        pr({ number: 3, state: "MERGED" }),
        pr({ number: 4, state: "CLOSED" }),
      ],
      linkedIssues: [
        { number: 10, state: "open", title: "a", url: "https://x/10" },
        { number: 11, state: "closed", title: "b", url: "https://x/11" },
      ],
    }),
    entry({
      worktreePath: "/m2",
      pullRequests: [
        pr({ number: 1, state: "OPEN" }),
        pr({ number: 5, state: "MERGED" }),
      ],
      linkedIssues: [
        { number: 10, state: "open", title: "a", url: "https://x/10" },
        { number: 12, state: "open", title: "c", url: "https://x/12" },
      ],
    }),
    entry({
      kind: "external",
      worktreePath: "/ext",
      pullRequests: [pr({ number: 99 })],
      linkedIssues: [
        { number: 99, state: "open", title: "x", url: "https://x/99" },
      ],
    }),
  ]);
  assert.deepEqual(counts, {
    prOpen: 1,
    prDraft: 1,
    prMerged: 2,
    prClosed: 1,
    issuesOpen: 2,
    issuesClosed: 1,
  });
});

test("aggregateGithubRollup empty registry", () => {
  assert.deepEqual(aggregateGithubRollup([]), {
    prOpen: 0,
    prDraft: 0,
    prMerged: 0,
    prClosed: 0,
    issuesOpen: 0,
    issuesClosed: 0,
  });
});

test("aggregateGithubRollup entries with no PRs or issues", () => {
  assert.deepEqual(
    aggregateGithubRollup([entry(), entry({ worktreePath: "/m2" })]),
    {
      prOpen: 0,
      prDraft: 0,
      prMerged: 0,
      prClosed: 0,
      issuesOpen: 0,
      issuesClosed: 0,
    },
  );
});
