import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  groupEvidenceTestEntries,
  parseEvidenceTestEntries,
} from "./evidenceTestEntries.ts";

describe("parseEvidenceTestEntries", () => {
  it("reads Failed/Passed bullet sections", () => {
    const entries = parseEvidenceTestEntries(`
Local suite finished.
Tests: 2 passed, 1 failed

Failed:
- auth.login rejects bad password

Passed:
- auth.login accepts owner
- channels.list returns open channels
`);
    assert.deepEqual(
      entries.map((entry) => `${entry.status}:${entry.name}`),
      [
        "failed:auth.login rejects bad password",
        "passed:auth.login accepts owner",
        "passed:channels.list returns open channels",
      ],
    );
  });

  it("reads markdown headings and status-prefixed lines", () => {
    const entries = parseEvidenceTestEntries(`
## Failing
### nested ignored without bullets

## Failed
- boom

## Passing
✓ desktop smoke
✗ should stay failed via prefix
`);
    // The ✗ line is outside a section after Passing was set... actually
    // "✗ should stay failed" matches STATUS_PREFIX_RE first and is failed.
    assert.ok(
      entries.some(
        (entry) => entry.status === "failed" && entry.name === "boom",
      ),
    );
    assert.ok(
      entries.some(
        (entry) => entry.status === "passed" && entry.name === "desktop smoke",
      ),
    );
    assert.ok(
      entries.some(
        (entry) =>
          entry.status === "failed" &&
          entry.name === "should stay failed via prefix",
      ),
    );
  });

  it("returns empty for count-only bodies", () => {
    assert.deepEqual(
      parseEvidenceTestEntries("Tests: 14 passed, 0 failed\nAll green."),
      [],
    );
  });

  it("groups without throwing on empty", () => {
    assert.deepEqual(groupEvidenceTestEntries([]), {
      failed: [],
      passed: [],
    });
  });
});
