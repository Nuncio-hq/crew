import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  formatCompactAge,
  formatFolderQuietAge,
  truncateMiddle,
} from "./workTreeFormat.ts";
import { WORK_TREE_QUIET_MS } from "./workTreeTypes.ts";

describe("formatCompactAge", () => {
  it("uses the spec compact tokens", () => {
    assert.equal(formatCompactAge(12 * 60_000), "12m");
    assert.equal(formatCompactAge(2 * 3_600_000), "2h");
    assert.equal(formatCompactAge(26 * 3_600_000), "1d");
  });
});

describe("formatFolderQuietAge", () => {
  it("only prints an age once the folder is quiet", () => {
    const now = 1_000_000_000_000;
    assert.equal(formatFolderQuietAge(now - 3_600_000, now), null);
    assert.equal(
      formatFolderQuietAge(now - WORK_TREE_QUIET_MS - 3_600_000, now),
      "2d",
    );
  });
});

describe("truncateMiddle", () => {
  it("keeps head and tail", () => {
    assert.equal(truncateMiddle("short", 28), "short");
    const long = "Fix the extremely long paywall crash on checkout";
    const out = truncateMiddle(long, 20);
    assert.ok(out.includes("…"));
    assert.ok(out.startsWith("Fix the"));
    assert.ok(out.endsWith("checkout".slice(-3)) || out.endsWith("out"));
  });
});
