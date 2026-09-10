import assert from "node:assert/strict";
import test from "node:test";
import {
  currentThreadTranscriptSession,
  isAgentTurnLive,
} from "./transcriptLiveScope.ts";

test("another thread's live turn cannot make retained thread history look live", () => {
  assert.equal(
    isAgentTurnLive([{ channelId: "channel", anchorAt: 1 }], "channel", []),
    false,
  );
});

test("exact current runs are live while channel compatibility remains unchanged", () => {
  const turns = [{ channelId: "channel", anchorAt: 1 }];
  assert.equal(isAgentTurnLive(turns, "channel"), true);
  assert.equal(isAgentTurnLive(turns, "other"), false);
  assert.equal(
    isAgentTurnLive(turns, "channel", [{ sessionId: "current" }]),
    true,
  );
});

test("historical or multiple live generations do not invent a single current session", () => {
  assert.equal(currentThreadTranscriptSession([]), null);
  assert.equal(
    currentThreadTranscriptSession([{ sessionId: "current" }]),
    "current",
  );
  assert.equal(
    currentThreadTranscriptSession([{ sessionId: "A" }, { sessionId: "B" }]),
    null,
  );
});
