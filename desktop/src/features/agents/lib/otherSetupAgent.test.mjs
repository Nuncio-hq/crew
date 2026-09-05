import assert from "node:assert/strict";
import test from "node:test";

import {
  isOtherSetupAgent,
  isOwnedAgentNotManagedOnDevice,
} from "./otherSetupAgent.ts";

const OWNER = "a".repeat(64);
const AGENT = "b".repeat(64);

test("fails closed while the local managed directory is unresolved", () => {
  assert.equal(
    isOtherSetupAgent({
      agentDirectoriesReady: false,
      currentPubkey: OWNER,
      managedAgents: [],
      profileOwnerPubkey: OWNER,
      pubkey: AGENT,
      relayAgents: [],
    }),
    false,
  );
});

test("labels a viewer-owned identity as not managed on this device", () => {
  assert.equal(
    isOtherSetupAgent({
      agentDirectoriesReady: true,
      currentPubkey: OWNER,
      managedAgents: [],
      profileOwnerPubkey: OWNER,
      pubkey: AGENT,
      relayAgents: [],
    }),
    true,
  );
});

test("falls back to the verified relay owner when the profile has no owner", () => {
  assert.equal(
    isOtherSetupAgent({
      agentDirectoriesReady: true,
      currentPubkey: OWNER,
      managedAgents: [],
      profileOwnerPubkey: null,
      pubkey: AGENT,
      relayAgents: [{ pubkey: AGENT.toUpperCase(), ownerPubkey: OWNER }],
    }),
    true,
  );
});

test("does not label a locally managed identity or another owner", () => {
  assert.equal(
    isOtherSetupAgent({
      agentDirectoriesReady: true,
      currentPubkey: OWNER,
      managedAgents: [{ pubkey: AGENT }],
      profileOwnerPubkey: OWNER,
      pubkey: AGENT,
      relayAgents: [],
    }),
    false,
  );
  assert.equal(
    isOtherSetupAgent({
      agentDirectoriesReady: true,
      currentPubkey: OWNER,
      managedAgents: [],
      profileOwnerPubkey: "c".repeat(64),
      pubkey: AGENT,
      relayAgents: [],
    }),
    false,
  );
});

test("a locally managed provider is not labeled as another device", () => {
  assert.equal(
    isOtherSetupAgent({
      agentDirectoriesReady: true,
      currentPubkey: OWNER,
      managedAgents: [{ pubkey: AGENT, backend: { type: "provider" } }],
      profileOwnerPubkey: OWNER,
      pubkey: AGENT,
      relayAgents: [],
    }),
    false,
  );
});

for (const [name, overrides, expected] of [
  ["owned absent key", {}, true],
  ["loading local inventory", { localInventoryReady: false }, false],
  ["exact local provider record", { isLocallyManaged: true }, false],
  ["different owner", { ownerPubkey: "b".repeat(64) }, false],
  ["unknown ownership", { ownerPubkey: null }, false],
]) {
  test(`shared provenance: ${name}`, () => {
    assert.equal(
      isOwnedAgentNotManagedOnDevice({
        currentPubkey: "a".repeat(64),
        ownerPubkey: "A".repeat(64),
        localInventoryReady: true,
        isLocallyManaged: false,
        ...overrides,
      }),
      expected,
    );
  });
}
