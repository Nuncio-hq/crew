import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  DIFF_FILE_TOLERANCE,
  DIFF_LINE_TOLERANCE_RATIO,
  compareEvidenceToPullRequest,
  parseEvidenceClaim,
  withinDiffFileTolerance,
  withinDiffLineTolerance,
} from "./evidenceCrossCheck.ts";

function pr(overrides = {}) {
  return {
    additions: 100,
    baseRefName: "main",
    changedFiles: 5,
    checks: [
      {
        name: "Crew CI",
        state: "SUCCESS",
        url: null,
        workflow: "Crew CI",
      },
    ],
    closingIssuesReferences: [],
    comments: [],
    deletions: 20,
    headRefName: "feat",
    isDraft: false,
    mergeStateStatus: "CLEAN",
    number: 9,
    reviewDecision: "REVIEW_REQUIRED",
    state: "OPEN",
    title: "Example",
    url: "https://example.test/9",
    ...overrides,
  };
}

describe("parseEvidenceClaim — test-run", () => {
  it("parses canonical Tests: N passed, M failed", () => {
    const claim = parseEvidenceClaim(
      "test-run",
      "Local suite:\nTests: 14 passed, 0 failed\nDone.",
    );
    assert.deepEqual(claim, {
      kind: "test-run",
      passed: 14,
      failed: 0,
      skipped: null,
    });
  });

  it("tolerates whitespace, ordering, and optional skipped", () => {
    assert.deepEqual(
      parseEvidenceClaim("test-run", "Tests:   0 failed ,  3 passed"),
      { kind: "test-run", passed: 3, failed: 0, skipped: null },
    );
    assert.deepEqual(
      parseEvidenceClaim("test-run", "Tests: 10 passed, 1 failed, 2 skipped"),
      { kind: "test-run", passed: 10, failed: 1, skipped: 2 },
    );
  });

  it("ignores prose that only looks like pass/fail (no Tests: line)", () => {
    assert.equal(
      parseEvidenceClaim(
        "test-run",
        "We saw 14 passed and 0 failed in the log excerpt",
      ),
      null,
    );
  });

  it("returns null for hostile/garbled Tests lines and never throws", () => {
    assert.equal(parseEvidenceClaim("test-run", "Tests: all green"), null);
    assert.equal(parseEvidenceClaim("test-run", "Tests: passed"), null);
    assert.equal(parseEvidenceClaim("test-run", ""), null);
    assert.equal(parseEvidenceClaim("test-run", null), null);
    assert.equal(parseEvidenceClaim("test-run", undefined), null);
    assert.equal(
      parseEvidenceClaim(
        "test-run",
        "Tests: 1 passed\nTests: also 99 failed somewhere",
      ),
      null,
    );
  });

  it("prefers the structured line when prose contradicts it", () => {
    const claim = parseEvidenceClaim(
      "test-run",
      "Everything failed spectacularly.\nTests: 14 passed, 0 failed\n0 passed, 99 failed",
    );
    assert.deepEqual(claim, {
      kind: "test-run",
      passed: 14,
      failed: 0,
      skipped: null,
    });
  });
});

describe("parseEvidenceClaim — diff-stat", () => {
  it("parses Diff: +A/−D across F files (unicode or ascii minus)", () => {
    assert.deepEqual(
      parseEvidenceClaim("diff-stat", "Diff: +120/−30 across 5 files"),
      { kind: "diff-stat", additions: 120, deletions: 30, files: 5 },
    );
    assert.deepEqual(
      parseEvidenceClaim("diff-stat", "Summary\nDiff: +8/-2 across 1 file\n"),
      { kind: "diff-stat", additions: 8, deletions: 2, files: 1 },
    );
  });

  it("parses Files: F | +A −D shorthand", () => {
    assert.deepEqual(parseEvidenceClaim("diff-stat", "Files: 4 | +42 −17"), {
      kind: "diff-stat",
      additions: 42,
      deletions: 17,
      files: 4,
    });
  });

  it("rejects near-miss shapes and free-form diff prose", () => {
    assert.equal(
      parseEvidenceClaim("diff-stat", "Diff: 120 additions, 30 deletions"),
      null,
    );
    assert.equal(parseEvidenceClaim("diff-stat", "Diff: +1/−2"), null);
  });
});

describe("parseEvidenceClaim — out of scope kinds", () => {
  it("metrics and before-after-visual never parse", () => {
    assert.equal(
      parseEvidenceClaim("metrics", "before: 1 | after: 2 | delta: 1"),
      null,
    );
    assert.equal(
      parseEvidenceClaim("before-after-visual", "https://example.test/a.png"),
      null,
    );
  });
});

describe("diff tolerance helpers", () => {
  it("pins ±10% lines and ±2 files", () => {
    assert.equal(DIFF_LINE_TOLERANCE_RATIO, 0.1);
    assert.equal(DIFF_FILE_TOLERANCE, 2);
    assert.equal(withinDiffLineTolerance(100, 100), true);
    assert.equal(withinDiffLineTolerance(110, 100), true);
    assert.equal(withinDiffLineTolerance(111, 100), false);
    assert.equal(withinDiffLineTolerance(0, 0), true);
    assert.equal(withinDiffLineTolerance(1, 0), false);
    assert.equal(withinDiffFileTolerance(5, 5), true);
    assert.equal(withinDiffFileTolerance(7, 5), true);
    assert.equal(withinDiffFileTolerance(8, 5), false);
  });
});

describe("compareEvidenceToPullRequest — test-run matrix", () => {
  it("green claim + green CI → Matches", () => {
    const result = compareEvidenceToPullRequest(
      "test-run",
      "Tests: 14 passed, 0 failed",
      pr(),
    );
    assert.equal(result.state, "matches");
    assert.equal(result.label, "Matches GitHub CI");
    assert.equal(result.detail, null);
  });

  it("claim-pass + CI-fail → Diverges with both values", () => {
    const result = compareEvidenceToPullRequest(
      "test-run",
      "Tests: 14 passed, 0 failed",
      pr({
        checks: [
          {
            name: "Desktop Fast",
            state: "FAILURE",
            url: null,
            workflow: "CI",
          },
        ],
      }),
    );
    assert.equal(result.state, "diverges");
    assert.match(result.detail, /Claimed: 14 passed, 0 failed/);
    assert.match(result.detail, /Desktop Fast — FAILURE/);
  });

  it("honestly-red claim + red CI → Matches (consistency)", () => {
    const result = compareEvidenceToPullRequest(
      "test-run",
      "Tests: 10 passed, 2 failed",
      pr({
        checks: [
          { name: "Crew CI", state: "FAILURE", url: null, workflow: null },
        ],
      }),
    );
    assert.equal(result.state, "matches");
  });

  it("honestly-red claim + green CI → Diverges", () => {
    const result = compareEvidenceToPullRequest(
      "test-run",
      "Tests: 10 passed, 2 failed",
      pr(),
    );
    assert.equal(result.state, "diverges");
  });

  it("pending checks → CI running", () => {
    const result = compareEvidenceToPullRequest(
      "test-run",
      "Tests: 14 passed, 0 failed",
      pr({
        checks: [
          { name: "Crew CI", state: "SUCCESS", url: null, workflow: null },
          {
            name: "Desktop E2E",
            state: "IN_PROGRESS",
            url: null,
            workflow: null,
          },
        ],
      }),
    );
    assert.equal(result.state, "ci-running");
    assert.equal(result.label, "GitHub CI running");
  });

  it("no PR / no checks / unparseable → Not comparable", () => {
    assert.equal(
      compareEvidenceToPullRequest(
        "test-run",
        "Tests: 1 passed, 0 failed",
        null,
      ).state,
      "not-comparable",
    );
    assert.equal(
      compareEvidenceToPullRequest(
        "test-run",
        "Tests: 1 passed, 0 failed",
        pr({ checks: [] }),
      ).state,
      "not-comparable",
    );
    assert.equal(
      compareEvidenceToPullRequest("test-run", "no structured line", pr())
        .state,
      "not-comparable",
    );
  });
});

describe("compareEvidenceToPullRequest — diff-stat matrix", () => {
  it("within tolerance → Matches", () => {
    const result = compareEvidenceToPullRequest(
      "diff-stat",
      "Diff: +105/−18 across 6 files",
      pr({ additions: 100, deletions: 20, changedFiles: 5 }),
    );
    assert.equal(result.state, "matches");
  });

  it("beyond tolerance → Diverges with both values", () => {
    const result = compareEvidenceToPullRequest(
      "diff-stat",
      "Diff: +120/−30 across 5 files",
      pr({ additions: 890, deletions: 4, changedFiles: 27 }),
    );
    assert.equal(result.state, "diverges");
    assert.match(result.detail, /Claimed: \+120\/−30 across 5 files/);
    assert.match(result.detail, /PR \+890\/−4 across 27 files/);
  });

  it("zero-file PR matches exact zero claim", () => {
    assert.equal(
      compareEvidenceToPullRequest(
        "diff-stat",
        "Diff: +0/−0 across 0 files",
        pr({ additions: 0, deletions: 0, changedFiles: 0, checks: [] }),
      ).state,
      "matches",
    );
  });

  it("absent PR → Not comparable", () => {
    assert.equal(
      compareEvidenceToPullRequest(
        "diff-stat",
        "Diff: +1/−0 across 1 files",
        null,
      ).state,
      "not-comparable",
    );
  });
});

describe("compareEvidenceToPullRequest — permanent Not comparable kinds", () => {
  it("metrics and before-after-visual stay Not comparable even with a green PR", () => {
    assert.equal(
      compareEvidenceToPullRequest(
        "metrics",
        "before: 1 | after: 2 | delta: +1",
        pr(),
      ).state,
      "not-comparable",
    );
    assert.equal(
      compareEvidenceToPullRequest(
        "before-after-visual",
        "https://example.test/a.png",
        pr(),
      ).state,
      "not-comparable",
    );
  });
});
