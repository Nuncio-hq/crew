import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  aggregateNeedsYou,
  needsYouKindHeading,
  needsYouSectionLabel,
} from "./needsYouAggregation.ts";

const ROOT = "a".repeat(64);
const CHANNEL = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";

describe("aggregateNeedsYou", () => {
  it("never counts the same id twice across kinds", () => {
    const result = aggregateNeedsYou([
      {
        channelId: CHANNEL,
        id: "req-1",
        kind: "question",
        threadRootId: ROOT,
        title: "Merge?",
      },
      {
        channelId: CHANNEL,
        id: "req-1",
        kind: "approval",
        threadRootId: ROOT,
        title: "Merge?",
      },
      {
        channelId: CHANNEL,
        id: "req-2",
        kind: "approval",
        threadRootId: ROOT,
        title: "PR #42",
      },
    ]);
    assert.equal(result.count, 2);
    assert.equal(result.grouped.question.length, 1);
    assert.equal(result.grouped.approval.length, 1);
    assert.equal(result.grouped.question[0].title, "Merge?");
  });

  it("keeps distinct ids in their own groups", () => {
    const result = aggregateNeedsYou([
      {
        channelId: CHANNEL,
        id: "q",
        kind: "question",
        threadRootId: ROOT,
        title: "Q",
      },
      {
        channelId: CHANNEL,
        id: "e",
        kind: "evidence",
        threadRootId: ROOT,
        title: "E",
      },
    ]);
    assert.equal(result.count, 2);
    assert.equal(needsYouSectionLabel(result.count), "Needs you · 2");
    assert.equal(needsYouKindHeading("question"), "Questions");
  });
});
