import assert from "node:assert/strict";
import test from "node:test";

import {
  activeFamilyStateIsCurrent,
  createDurableProjectionFamilyCounters,
  eventBelongsToActiveProjection,
} from "./durableProjectionFamily.ts";

test("family generation checks ignore unrelated projection retries", () => {
  const current = createDurableProjectionFamilyCounters();
  const snapshot = { ...current };
  current.receipt += 1;

  assert.equal(
    activeFamilyStateIsCurrent(current, snapshot, {
      receipt: false,
      userInput: true,
    }),
    true,
  );
  assert.equal(
    activeFamilyStateIsCurrent(current, snapshot, {
      receipt: true,
      userInput: false,
    }),
    false,
  );
});

test("failed-family overlap remains outside a healthy family hydration", () => {
  const receipt = { id: "receipt", kind: 46043 };
  const userInput = { id: "input", kind: 46040 };
  const familyForEvent = (event) =>
    event.kind === 46043
      ? "receipt"
      : event.kind === 46040
        ? "userInput"
        : null;
  const active = { receipt: true, userInput: false };

  assert.equal(
    eventBelongsToActiveProjection(receipt, familyForEvent, active),
    true,
  );
  assert.equal(
    eventBelongsToActiveProjection(userInput, familyForEvent, active),
    false,
  );
});
