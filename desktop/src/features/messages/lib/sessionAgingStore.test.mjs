import assert from "node:assert/strict";
import { describe, it, beforeEach } from "node:test";

import {
  clearAllSessionAging,
  clearSessionAging,
  getSessionAging,
  parseSessionAgingPayload,
  putSessionAging,
  sessionAgingBannerText,
} from "./sessionAgingStore.test-support.mjs";

describe("sessionAgingStore (#173)", () => {
  beforeEach(() => {
    clearAllSessionAging();
  });

  it("stores aging only when aging=true and never fabricates unknown counts", () => {
    putSessionAging({
      agentPubkey: "aa".repeat(32),
      channelId: "chan-1",
      conversationId: "chan-1",
      aging: true,
      reason: "turn_count_net",
      compactionCount: 9,
      compactionSignal: "unknown",
      sessionTurnCount: 100,
      compactionThreshold: 3,
      turnThreshold: 100,
    });
    const entry = getSessionAging("aa".repeat(32), "chan-1");
    assert.ok(entry);
    assert.equal(entry.compactionCount, 0);
    assert.match(sessionAgingBannerText(entry, "Grok"), /100\+ turns/);
    assert.doesNotMatch(sessionAgingBannerText(entry, "Grok"), /compacted/);
  });

  it("shows compaction count only when signal is known", () => {
    putSessionAging({
      agentPubkey: "bb".repeat(32),
      channelId: "chan-2",
      conversationId: "chan-2",
      aging: true,
      reason: "compaction_threshold",
      compactionCount: 3,
      compactionSignal: "known",
      sessionTurnCount: 12,
      compactionThreshold: 3,
      turnThreshold: 100,
    });
    const entry = getSessionAging("bb".repeat(32), "chan-2");
    assert.ok(entry);
    assert.equal(entry.compactionCount, 3);
    assert.match(
      sessionAgingBannerText(entry, "Hermes"),
      /session compacted 3×/,
    );
  });

  it("clears on reset", () => {
    putSessionAging({
      agentPubkey: "cc".repeat(32),
      channelId: "chan-3",
      conversationId: "chan-3",
      aging: true,
      reason: "compaction_threshold",
      compactionCount: 3,
      compactionSignal: "known",
      sessionTurnCount: 3,
      compactionThreshold: 3,
      turnThreshold: 100,
    });
    clearSessionAging("cc".repeat(32), "chan-3");
    assert.equal(getSessionAging("cc".repeat(32), "chan-3"), undefined);
  });

  it("parses harness session_aging payload", () => {
    const parsed = parseSessionAgingPayload("dd".repeat(32), {
      channelId: "chan-4",
      conversationId: "chan-4",
      aging: true,
      reason: "compaction_threshold",
      compactionCount: 3,
      compactionSignal: "known",
      sessionTurnCount: 8,
      compactionThreshold: 3,
      turnThreshold: 100,
    });
    assert.ok(parsed);
    assert.equal(parsed.aging, true);
    assert.equal(parsed.compactionCount, 3);
  });
});
