import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  formatAbsenceBanner,
  formatObservedIdleLine,
  formatWallAge,
  repositoryLabel,
} from "./formatObservedIdle.ts";

describe("formatObservedIdle", () => {
  it("formats dual clocks", () => {
    assert.equal(
      formatObservedIdleLine({
        observedIdleSecs: 52 * 3600,
        wallIdleSecs: 9 * 86_400,
      }),
      "idle 52 observed hrs (last used 9 days ago)",
    );
  });

  it("formats absence banner only for multi-day gaps", () => {
    assert.equal(formatAbsenceBanner(86_400), null);
    assert.equal(
      formatAbsenceBanner(6 * 86_400),
      "You were away 6 days — in-progress threads are unlikely to qualify yet",
    );
  });

  it("formats short wall ages", () => {
    assert.equal(formatWallAge(90), "1 min");
    assert.equal(formatWallAge(7200), "2 hours");
  });

  it("labels repository path by basename", () => {
    assert.equal(repositoryLabel("/Users/me/src/crew/"), "crew");
  });
});
