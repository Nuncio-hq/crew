import assert from "node:assert/strict";
import test from "node:test";
import { createUserInputAnswerGate } from "./userInputAnswerGate.ts";

test("bounded pending confirmation offers an error and recovers after reconciliation", () => {
  const gate = createUserInputAnswerGate({});
  const requests = Array.from({ length: 129 }, (_, i) => ({
    event: { id: String(i) },
    request: { channel_id: "channel" },
  }));
  gate.reconcile(requests);
  for (const request of requests.slice(0, 128)) {
    const claim = gate.claim(request.event.id);
    assert.ok(claim);
    gate.accept(claim);
    gate.release(claim);
  }
  assert.equal(
    typeof gate.claim("128")?.error,
    "string",
    "capacity must not turn Answer into a silent no-op",
  );
  assert.equal(gate.claim("0"), null, "accepted answer cannot replay");
  gate.reconcile(requests.slice(1));
  assert.equal(
    gate.claim("128")?.request.event.id,
    "128",
    "confirmed removal frees a slot",
  );
});
