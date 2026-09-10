import assert from "node:assert/strict";
import test from "node:test";
import {
  managedAgentTransportPresentation,
  managedAgentProcessLabel,
} from "./managedAgentTransportStatus.ts";

function runtime(state, overrides = {}) {
  return {
    pubkey: "a".repeat(64),
    relayUrl: "ws://fixture",
    localSetup: true,
    lifecycle: "ready",
    pid: 100,
    error: null,
    logPath: null,
    transport: {
      state,
      code: "none",
      attempts: 3,
      elapsedMs: 2000,
      nextRetryAtMs: null,
      lastError: null,
    },
    transportRetired: false,
    ...overrides,
  };
}

test("all local transport states have distinct labels and recovery", () => {
  const labels = new Set();
  for (const state of [
    "unknown",
    "connecting",
    "connected",
    "degraded",
    "exhausted",
    "auth_rejected",
  ]) {
    const presentation = managedAgentTransportPresentation(runtime(state));
    assert.ok(presentation, state);
    labels.add(presentation.label);
    assert.equal(presentation.needsRetry, state !== "connected", state);
  }
  assert.equal(labels.size, 6);
});

test("offline live process stays visibly running instead of stopped", () => {
  const value = runtime("exhausted");
  assert.equal(managedAgentProcessLabel(value), "Running locally");
  assert.equal(value.lifecycle, "ready");
  assert.equal(value.pid, 100);
});

test("retired diagnostics are labeled as previous process without claiming live probes", () => {
  const presentation = managedAgentTransportPresentation(
    runtime("auth_rejected", {
      lifecycle: "failed",
      pid: null,
      transportRetired: true,
    }),
  );
  assert.match(presentation?.label ?? "", /Last process/);
  assert.equal(presentation?.needsRetry, true);
  assert.doesNotMatch(presentation?.detail ?? "", /probes continue/i);
});

test("auth denial explains review while transport exhaustion never claims bad credentials", () => {
  assert.match(
    managedAgentTransportPresentation(runtime("auth_rejected"))?.detail ?? "",
    /credentials|configuration/i,
  );
  assert.doesNotMatch(
    managedAgentTransportPresentation(runtime("exhausted"))?.detail ?? "",
    /credentials|authentication denied/i,
  );
});

test("connected recovery clears retry and restores normal process label", () => {
  assert.equal(
    managedAgentTransportPresentation(runtime("connected"))?.needsRetry,
    false,
  );
  assert.equal(managedAgentProcessLabel(runtime("connected")), "Here");
});

test("legacy payload and setup state do not invent an observed transport", () => {
  assert.equal(managedAgentTransportPresentation(undefined), null);
  assert.equal(
    managedAgentTransportPresentation(
      runtime("unknown", { transport: undefined }),
    ),
    null,
  );
  assert.equal(
    managedAgentTransportPresentation(runtime("unknown", { localSetup: false }))
      ?.needsRetry ?? false,
    false,
  );
});
