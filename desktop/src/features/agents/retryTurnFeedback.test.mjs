import assert from "node:assert/strict";
import test from "node:test";

import {
  describeRetryTurnResult,
  normalizeRetryTurnStatus,
} from "./retryTurnFeedback.ts";

test("normalizeRetryTurnStatus keeps known statuses", () => {
  assert.equal(normalizeRetryTurnStatus("dispatched"), "dispatched");
  assert.equal(normalizeRetryTurnStatus("agent_removed"), "agent_removed");
  assert.equal(normalizeRetryTurnStatus("nope"), "unconfirmed");
});

test("describeRetryTurnResult gives distinct copy per status", () => {
  const statuses = [
    "dispatched",
    "dispatched_partial",
    "events_gone",
    "already_running",
    "agent_removed",
    "unconfirmed",
  ];
  const messages = new Set(
    statuses.map((status) => describeRetryTurnResult(status, "Bee").message),
  );
  assert.equal(messages.size, statuses.length);
});
