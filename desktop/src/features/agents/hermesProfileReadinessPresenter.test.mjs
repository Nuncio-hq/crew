import assert from "node:assert/strict";
import test from "node:test";

import { presentHermesProfileReadiness } from "./hermesProfileReadinessPresenter.ts";

test("presents every Hermes readiness state with stable copy and test id", () => {
  const states = [
    { state: "ready" },
    { state: "missing", profile: "scout" },
    { state: "broken_config", profile: "scout", diagnostic: "bad yaml" },
    { state: "binary_missing", command: "hermes" },
    { state: "auth_unknown", profile: "scout" },
  ];

  for (const state of states) {
    const presentation = presentHermesProfileReadiness(state);
    assert.ok(presentation.label);
    assert.ok(presentation.explanation);
    assert.ok(presentation.repair);
    assert.match(presentation.testId, /^hermes-readiness-/);
  }
});

test("auth unknown is neutral and explicitly not verifiable", () => {
  const presentation = presentHermesProfileReadiness({
    state: "auth_unknown",
    profile: "scout",
  });
  assert.equal(presentation.tone, "neutral");
  assert.match(presentation.explanation, /cannot be verified/i);
  assert.equal(presentation.label, "Auth not verifiable");
});
