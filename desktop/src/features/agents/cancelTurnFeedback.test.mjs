import assert from "node:assert/strict";
import test from "node:test";

import {
  describeCancelTurnResult,
  normalizeCancelTurnStatus,
  pickStrongestCancelTurnStatus,
} from "./cancelTurnFeedback.ts";

test("each control_result status maps to a distinct message", () => {
  const sent = describeCancelTurnResult("sent", "Honey");
  const queued = describeCancelTurnResult("cancelled_queued", "Honey");
  const idle = describeCancelTurnResult("no_active_turn", "Honey");
  assert.notEqual(sent.message, queued.message);
  assert.notEqual(sent.message, idle.message);
  assert.notEqual(queued.message, idle.message);
  assert.equal(sent.tone, "info");
  assert.equal(queued.tone, "success");
  assert.equal(idle.tone, "warning");
});

test("unknown status becomes unconfirmed", () => {
  assert.equal(normalizeCancelTurnStatus("weird"), "unconfirmed");
});

test("strongest status prefers sent over queued over idle", () => {
  assert.equal(
    pickStrongestCancelTurnStatus([
      "no_active_turn",
      "cancelled_queued",
      "sent",
    ]),
    "sent",
  );
  assert.equal(
    pickStrongestCancelTurnStatus(["no_active_turn", "cancelled_queued"]),
    "cancelled_queued",
  );
});
