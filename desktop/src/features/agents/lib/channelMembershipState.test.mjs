import assert from "node:assert/strict";
import test from "node:test";
import {
  applyChannelMembershipObserverFrame,
  getChannelMembershipState,
  hasNoChannelMembership,
  resetChannelMembershipState,
  deriveNoChannelMembershipBadge,
} from "./channelMembershipState.ts";
import { applyCrewLiveFrameSideEffects } from "../observerRelayStoreCrew.ts";
const agent = "a".repeat(64);
const frame = (
  count,
  seq = 1,
  generation = "one",
  start = "2026-09-05T00:00:00Z",
) => ({
  kind: "channel_membership",
  seq,
  payload: { channel_count: count, generation, generation_started_at: start },
});
test.beforeEach(resetChannelMembershipState);
test("membership loss and recovery update without restart", () => {
  applyChannelMembershipObserverFrame(agent, frame(0));
  assert.equal(hasNoChannelMembership(agent), true);
  applyChannelMembershipObserverFrame(agent, frame(2, 2));
  assert.equal(hasNoChannelMembership(agent), false);
  applyChannelMembershipObserverFrame(agent, frame(0, 3));
  assert.equal(hasNoChannelMembership(agent), true);
});
test("old generations and reordered frames cannot overwrite current membership", () => {
  applyChannelMembershipObserverFrame(agent, frame(0));
  applyChannelMembershipObserverFrame(
    agent,
    frame(2, 1, "two", "2026-09-05T01:00:00Z"),
  );
  applyChannelMembershipObserverFrame(agent, frame(0, 99));
  assert.equal(hasNoChannelMembership(agent), false);
  applyChannelMembershipObserverFrame(
    agent,
    frame(0, 0, "two", "2026-09-05T01:00:00Z"),
  );
  assert.equal(hasNoChannelMembership(agent), false);
});
test("community reset clears projection and generation authority", () => {
  applyChannelMembershipObserverFrame(agent, frame(0));
  resetChannelMembershipState();
  assert.equal(hasNoChannelMembership(agent), false);
  applyChannelMembershipObserverFrame(agent, frame(0));
  assert.equal(hasNoChannelMembership(agent), true);
});
test("invalid payloads cannot manufacture no-channel readiness", () => {
  for (const count of [-1, NaN, 0.5, "0", null])
    applyChannelMembershipObserverFrame(agent, frame(count));
  applyChannelMembershipObserverFrame(agent, {
    kind: "channel_membership",
    seq: 1,
    payload: { channel_count: 0 },
  });
  assert.equal(hasNoChannelMembership(agent), false);
});
test("explicitly unknown membership never becomes confirmed zero", () => {
  applyChannelMembershipObserverFrame(agent, frame(null));
  assert.equal(getChannelMembershipState(agent), "unknown");
  assert.equal(hasNoChannelMembership(agent), false);
  applyChannelMembershipObserverFrame(agent, frame(0, 2));
  assert.equal(getChannelMembershipState(agent), "zero");
  assert.equal(hasNoChannelMembership(agent), true);
  applyChannelMembershipObserverFrame(agent, frame(null, 3));
  assert.equal(getChannelMembershipState(agent), "unknown");
  assert.equal(hasNoChannelMembership(agent), false);
});
test("observer dispatch preserves unknown membership state", () => {
  applyCrewLiveFrameSideEffects(agent, frame(null));
  assert.equal(getChannelMembershipState(agent), "unknown");
  applyCrewLiveFrameSideEffects(agent, frame(0, 2));
  assert.equal(getChannelMembershipState(agent), "zero");
});
test("no-channel status is independent of working and hidden when stopped", () => {
  assert.equal(deriveNoChannelMembershipBadge(true, "running"), true);
  assert.equal(deriveNoChannelMembershipBadge(true, "deployed"), true);
  assert.equal(deriveNoChannelMembershipBadge(true, "stopped"), false);
  assert.equal(deriveNoChannelMembershipBadge(false, "running"), false);
});

test("generation identity cannot change its start timestamp to roll back authority", () => {
  applyChannelMembershipObserverFrame(agent, frame(2));
  applyChannelMembershipObserverFrame(
    agent,
    frame(0, 2, "one", "2026-09-05T03:00:00Z"),
  );
  assert.equal(hasNoChannelMembership(agent), false);
  applyChannelMembershipObserverFrame(
    agent,
    frame(0, 3, "new", "invalid timestamp"),
  );
  assert.equal(hasNoChannelMembership(agent), false);
});

test("agent keys normalize and unrelated agent snapshots stay separate", () => {
  applyChannelMembershipObserverFrame(agent.toUpperCase(), frame(0));
  applyChannelMembershipObserverFrame("b".repeat(64), frame(2));
  assert.equal(hasNoChannelMembership(agent), true);
  assert.equal(hasNoChannelMembership("b".repeat(64)), false);
});
