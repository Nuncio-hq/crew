import assert from "node:assert/strict";
import { test } from "node:test";

import { buildProjectThreadBadge } from "./projectThreadBadge.ts";

function entry(overrides = {}) {
  return {
    worktreePath: "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
    worktreeName: "crew-aaaaaaaaaaaa",
    branch: "buzz/aaaaaaaaaaaa",
    head: "deadbeef",
    kind: "managed",
    rootEventId: "a".repeat(64),
    prunable: false,
    pullRequests: [],
    linkedIssues: [],
    ...overrides,
  };
}

function pr(number, state, extras = {}) {
  return {
    number,
    state,
    isDraft: false,
    reviewDecision: "",
    checks: "none",
    additions: 10,
    deletions: 2,
    title: `PR ${number}`,
    url: `https://example.test/${number}`,
    ...extras,
  };
}

function issue(number, state, title = `Issue ${number}`) {
  return {
    number,
    state,
    title,
    url: `https://example.test/issues/${number}`,
  };
}

test("entry without PR → worktree chip only", () => {
  const badge = buildProjectThreadBadge(entry(), "brainstorm worktrees");
  assert.equal(badge.pullRequests.length, 0);
  assert.equal(badge.overflow, 0);
  assert.equal(badge.diff, null);
  assert.equal(badge.openIssues, null);
  assert.equal(badge.label, "brainstorm worktrees");
  assert.equal(badge.mono, false);
});

test("open PR with green rollup → check glyph and diff", () => {
  const badge = buildProjectThreadBadge(
    entry({
      pullRequests: [
        pr(42, "OPEN", { checks: "passing", additions: 12, deletions: 3 }),
      ],
    }),
    null,
  );
  assert.equal(badge.pullRequests[0].number, 42);
  assert.equal(badge.pullRequests[0].checkGlyph, "✓");
  assert.equal(badge.pullRequests[0].tone, "success");
  assert.deepEqual(badge.diff, { additions: 12, deletions: 3 });
  assert.equal(badge.mono, true);
  assert.equal(badge.shortBranch, "aaaaaaaa");
});

test("draft PR → no check glyph, draft tone", () => {
  const badge = buildProjectThreadBadge(
    entry({
      pullRequests: [pr(7, "OPEN", { isDraft: true, checks: "pending" })],
    }),
    "label",
  );
  assert.equal(badge.pullRequests[0].checkGlyph, null);
  assert.equal(badge.pullRequests[0].tone, "draft");
});

test("four PRs → exactly two chips plus overflow 2, open first", () => {
  const badge = buildProjectThreadBadge(
    entry({
      pullRequests: [
        pr(12, "OPEN"),
        pr(13, "OPEN", { isDraft: true }),
        pr(11, "MERGED"),
        pr(10, "CLOSED"),
      ],
    }),
    "x",
  );
  assert.deepEqual(
    badge.pullRequests.map((item) => item.number),
    [12, 13],
  );
  assert.equal(badge.overflow, 2);
});

test("empty pull list (github unavailable) → worktree only", () => {
  const badge = buildProjectThreadBadge(entry({ pullRequests: [] }), "x");
  assert.equal(badge.pullRequests.length, 0);
  assert.equal(badge.diff, null);
  assert.equal(badge.openIssues, null);
});

test("open linked issues → ◉ count chip with tooltip titles", () => {
  const badge = buildProjectThreadBadge(
    entry({
      linkedIssues: [
        issue(5, "open", "Wire rollup"),
        issue(6, "closed", "Done"),
        issue(7, "open", "Counts"),
      ],
    }),
    "x",
  );
  assert.equal(badge.openIssues?.openCount, 2);
  assert.equal(badge.openIssues?.title, "#5 Wire rollup\n#7 Counts");
});

test("zero open issues → no issue chip", () => {
  const badge = buildProjectThreadBadge(
    entry({ linkedIssues: [issue(1, "closed")] }),
    "x",
  );
  assert.equal(badge.openIssues, null);
});

test("non-managed entry returns null", () => {
  assert.equal(buildProjectThreadBadge(entry({ kind: "external" }), "x"), null);
  assert.equal(buildProjectThreadBadge(entry({ branch: null }), "x"), null);
});
