import assert from "node:assert/strict";
import test from "node:test";

import {
  deriveNoChannelMembershipBadge,
  getChannelMembershipState,
  hasNoChannelMembership,
  observeChannelMembershipRuntime,
  resetChannelMembershipState,
  subscribeChannelMembershipState,
} from "./channelMembershipState.ts";
import {
  _testProcessLiveObserverEvents,
  resetAgentObserverStore,
} from "../observerRelayStore.ts";

const agent = "a".repeat(64);
const otherAgent = "b".repeat(64);
const relayUrl = "wss://relay.example";

function runtime(startNonce = "one", overrides = {}) {
  return {
    pubkey: agent,
    relayUrl,
    startNonce,
    transport: { state: "connected" },
    transportRetired: false,
    ...overrides,
  };
}

function frame(
  count,
  seq = 1,
  generation = "one",
  started = "2026-09-05T00:00:00Z",
) {
  return {
    kind: "channel_membership",
    seq,
    timestamp: new Date(Date.parse(started) + seq * 1_000).toISOString(),
    agentIndex: null,
    channelId: null,
    sessionId: null,
    turnId: null,
    payload: {
      channel_count: count,
      generation,
      generation_started_at: started,
    },
  };
}

function ingest(event, agentPubkey = agent) {
  _testProcessLiveObserverEvents(agentPubkey, [event]);
}

test.beforeEach(() => {
  resetAgentObserverStore();
  resetChannelMembershipState();
});

test("membership loss and recovery update through the live observer ingress", () => {
  ingest(frame(0));
  assert.equal(getChannelMembershipState(agent, runtime()), "zero");
  ingest(frame(2, 2));
  assert.equal(hasNoChannelMembership(agent, runtime()), false);
  ingest(frame(0, 3));
  assert.equal(hasNoChannelMembership(agent, runtime()), true);
});

test("old generations cannot overwrite the matching current runtime", () => {
  ingest(frame(0, 1, "old"));
  ingest(frame(2, 1, "current", "2026-09-05T01:00:00Z"));
  observeChannelMembershipRuntime(agent, runtime("current"));
  ingest(frame(0, 99, "old"));
  assert.equal(getChannelMembershipState(agent, runtime("current")), "nonzero");
});

test("a new generation notifies even when its count is unchanged", () => {
  let notifications = 0;
  const unsubscribe = subscribeChannelMembershipState(() => {
    notifications += 1;
  });
  ingest(frame(0, 1, "one"));
  ingest(frame(0, 1, "two", "2026-09-05T01:00:00Z"));
  ingest(frame(0, 2, "two", "2026-09-05T01:00:00Z"));
  unsubscribe();
  assert.equal(notifications, 2);
});

test("bounded generation history keeps late evicted frames from poisoning a live row", () => {
  for (let index = 0; index <= 130; index += 1) {
    const generation = `generation-${index}`;
    ingest(
      frame(
        index === 130 ? 2 : 1,
        index + 1,
        generation,
        new Date(Date.UTC(2026, 0, 1) + index * 60_000).toISOString(),
      ),
    );
  }
  observeChannelMembershipRuntime(agent, runtime("generation-130"));

  // generation-0 is outside the bounded retained window. A delayed frame for
  // it may be accepted into the journal, but it must not replace generation-130.
  ingest(frame(0, 10_000, "generation-0", "2027-01-01T00:00:00Z"));
  assert.equal(
    getChannelMembershipState(agent, runtime("generation-130")),
    "nonzero",
  );
  assert.equal(
    getChannelMembershipState(agent, runtime("generation-0")),
    "unknown",
  );
});

test("fresh generations recover after bounded tombstones recycle", () => {
  for (let index = 0; index <= 260; index += 1) {
    const generation = `generation-${index}`;
    observeChannelMembershipRuntime(agent, runtime(generation));
    ingest(
      frame(
        index === 260 ? 2 : 1,
        index + 1,
        generation,
        new Date(Date.UTC(2026, 0, 1) + index * 60_000).toISOString(),
      ),
    );
  }

  assert.equal(
    getChannelMembershipState(agent, runtime("generation-260")),
    "nonzero",
  );
});

test("late old generations cannot evict the active row by arrival order", () => {
  ingest(frame(2, 1, "current", "2026-09-05T04:00:00Z"));
  // The observer frame is intentionally ahead of the native status. The
  // pending candidate must survive a burst of delayed prior generations.
  for (let index = 0; index <= 130; index += 1) {
    ingest(
      frame(
        0,
        index + 2,
        `old-${index}`,
        new Date(Date.UTC(2026, 0, 1) + index * 60_000).toISOString(),
      ),
    );
  }
  observeChannelMembershipRuntime(agent, runtime("current"));
  assert.equal(getChannelMembershipState(agent, runtime("current")), "nonzero");
});

test("a connected runtime retires its predecessor and fences replay", () => {
  observeChannelMembershipRuntime(agent, runtime("old"));
  ingest(frame(0, 1, "old"));
  observeChannelMembershipRuntime(agent, runtime("current"));
  ingest(frame(2, 1, "old"));
  assert.equal(getChannelMembershipState(agent, runtime("old")), "unknown");
  assert.equal(getChannelMembershipState(agent, runtime("current")), "unknown");
  ingest(frame(2, 2, "current", "2026-09-05T01:00:00Z"));
  assert.equal(getChannelMembershipState(agent, runtime("current")), "nonzero");
});

test("repeated stale runtime reads do not discard a pending restart frame", () => {
  observeChannelMembershipRuntime(agent, runtime("old"));
  ingest(frame(0, 1, "old"));
  ingest(frame(2, 1, "current", "2026-09-05T01:00:00Z"));
  observeChannelMembershipRuntime(agent, runtime("old"));
  observeChannelMembershipRuntime(agent, runtime("current"));
  assert.equal(getChannelMembershipState(agent, runtime("current")), "nonzero");
});

test("community reset clears projection and generation authority", () => {
  ingest(frame(0));
  assert.equal(getChannelMembershipState(agent, runtime()), "zero");
  resetChannelMembershipState();
  assert.equal(getChannelMembershipState(agent, runtime()), "unknown");
  ingest(frame(0));
  assert.equal(getChannelMembershipState(agent, runtime()), "zero");
});

test("missing, disconnected, retired, and mismatched runtimes stay unknown", () => {
  ingest(frame(0));
  assert.equal(getChannelMembershipState(agent, undefined), "unknown");
  assert.equal(
    getChannelMembershipState(agent, runtime("one", { startNonce: undefined })),
    "unknown",
  );
  assert.equal(
    getChannelMembershipState(
      agent,
      runtime("one", { transport: { state: "error" } }),
    ),
    "unknown",
  );
  assert.equal(
    getChannelMembershipState(
      agent,
      runtime("one", { transportRetired: true }),
    ),
    "unknown",
  );
  assert.equal(
    getChannelMembershipState(agent, runtime("other-generation")),
    "unknown",
  );
  assert.equal(
    getChannelMembershipState(otherAgent, runtime("one")),
    "unknown",
  );
});

test("unknown signal never becomes confirmed zero and recovers on a later count", () => {
  ingest(frame(null));
  assert.equal(getChannelMembershipState(agent, runtime()), "unknown");
  assert.equal(hasNoChannelMembership(agent, runtime()), false);
  ingest(frame(0, 2));
  assert.equal(getChannelMembershipState(agent, runtime()), "zero");
  assert.equal(hasNoChannelMembership(agent, runtime()), true);
  ingest(frame(null, 3));
  assert.equal(getChannelMembershipState(agent, runtime()), "unknown");
});

test("invalid payloads cannot manufacture no-channel readiness", () => {
  for (const [index, count] of [-1, Number.NaN, 0.5, "0"].entries()) {
    ingest(frame(count, index + 1));
  }
  ingest({
    ...frame(0),
    payload: { channel_count: 0 },
  });
  assert.equal(hasNoChannelMembership(agent, runtime()), false);
});

test("same-generation reordered frames are ignored", () => {
  ingest(frame(2, 2));
  ingest(frame(0, 1));
  assert.equal(getChannelMembershipState(agent, runtime()), "nonzero");
});

test("a generation cannot rewrite its start timestamp", () => {
  ingest(frame(2, 1, "one", "2026-09-05T00:00:00Z"));
  ingest(frame(0, 2, "one", "2026-09-05T03:00:00Z"));
  assert.equal(getChannelMembershipState(agent, runtime()), "nonzero");
  ingest({
    ...frame(0, 3, "new"),
    payload: {
      channel_count: 0,
      generation: "new",
      generation_started_at: "invalid timestamp",
    },
  });
  assert.equal(getChannelMembershipState(agent, runtime()), "nonzero");
});

test("no-channel status is independent of working and hidden when stopped", () => {
  assert.equal(deriveNoChannelMembershipBadge(true, "running"), true);
  assert.equal(deriveNoChannelMembershipBadge(true, "deployed"), true);
  assert.equal(deriveNoChannelMembershipBadge(true, "stopped"), false);
  assert.equal(deriveNoChannelMembershipBadge(false, "running"), false);
});

test("agent keys normalize and unrelated agent snapshots stay separate", () => {
  ingest(frame(0), agent.toUpperCase());
  ingest(frame(2), otherAgent);
  assert.equal(getChannelMembershipState(agent, runtime()), "zero");
  assert.equal(
    getChannelMembershipState(
      otherAgent,
      runtime("one", { pubkey: otherAgent }),
    ),
    "nonzero",
  );
});
