import assert from "node:assert/strict";
import { beforeEach, describe, it } from "node:test";

import {
  getAgentObserverSnapshot,
  getAgentTranscript,
  getArchivedChannelEvents,
  ingestArchivedObserverEvents,
  pruneIdleAgentObserverData,
  resetAgentObserverStore,
  subscribeAgentObserverProjection,
  subscribeAgentObserverStore,
  syncAgentObserverEvents,
  _testRegisterKnownAgents,
} from "@/features/agents/observerRelayStore.ts";

const AGENT_PUBKEY = "a".repeat(64);
const OTHER_PUBKEY = "b".repeat(64);
const SUBSCRIPTION_ID = "retention-test";
const ARCHIVE_EVENT_CAP = 3000;
const ARCHIVE_CHANNEL_BUDGET = 12;
const IDLE_RETENTION_MS = 5 * 60_000;

function makeRawEvent(index, channelId = "channel-1") {
  return {
    id: index.toString(16).padStart(64, "0"),
    pubkey: AGENT_PUBKEY,
    created_at: 1000 + index,
    kind: 24200,
    tags: [
      ["p", OTHER_PUBKEY],
      ["agent", AGENT_PUBKEY],
      ["frame", "telemetry"],
      ["h", channelId],
    ],
    content: "encrypted",
    sig: "s".repeat(128),
  };
}

function makeObserverEvent(index, channelId = "channel-1", overrides = {}) {
  return {
    seq: index,
    timestamp: new Date(1_000_000 + index * 1000).toISOString(),
    kind: "turn_started",
    agentIndex: 0,
    channelId,
    conversationId: channelId,
    sessionId: "session-1",
    turnId: `turn-${index}`,
    payload: {},
    ...overrides,
  };
}

async function ingestEvents(events) {
  let index = 0;
  await ingestArchivedObserverEvents(
    events.map((event) => makeRawEvent(event.seq, event.channelId)),
    () => Promise.resolve(events[index++]),
  );
}

describe("observer archive retention", () => {
  beforeEach(() => {
    resetAgentObserverStore();
    _testRegisterKnownAgents(SUBSCRIPTION_ID, [AGENT_PUBKEY]);
  });

  it("caps one archive key, preserves causal order, and deduplicates at the boundary", async () => {
    const total = ARCHIVE_EVENT_CAP + 100;
    const events = Array.from({ length: total }, (_, index) =>
      makeObserverEvent(index + 1),
    );
    events.push(makeObserverEvent(101));

    await ingestEvents(events);

    const retained = getArchivedChannelEvents(AGENT_PUBKEY, "channel-1");
    assert.equal(retained.length, ARCHIVE_EVENT_CAP);
    assert.equal(retained[0]?.seq, 101);
    assert.equal(retained.at(-1)?.seq, total);
    assert.equal(
      retained.filter((event) => event.seq === 101).length,
      1,
      "a duplicate at the eviction boundary must not consume the window",
    );
  });

  it("evicts the least-recently-read archive key and notifies once", async () => {
    for (let index = 0; index < ARCHIVE_CHANNEL_BUDGET; index++) {
      const channelId = `channel-${String.fromCharCode(65 + index)}`;
      await ingestEvents([makeObserverEvent(index + 1, channelId)]);
    }

    getArchivedChannelEvents(AGENT_PUBKEY, "channel-A");
    let notifications = 0;
    const unsubscribe = subscribeAgentObserverStore(() => {
      notifications++;
    });

    await ingestEvents([makeObserverEvent(100, "channel-M")]);
    unsubscribe();

    assert.equal(
      getArchivedChannelEvents(AGENT_PUBKEY, "channel-A").length,
      1,
      "the recently-read key must survive",
    );
    const evicted = getArchivedChannelEvents(AGENT_PUBKEY, "channel-B");
    assert.equal(
      evicted.length,
      0,
      "the least-recently-read key must be evicted",
    );
    assert.equal(
      evicted,
      getArchivedChannelEvents(AGENT_PUBKEY, "channel-missing"),
      "evicted keys must return the shared empty result",
    );
    assert.equal(notifications, 1, "append and eviction notify as one batch");
  });

  it("sorts a 1000-event archive page once", async () => {
    const pageSize = 1000;
    const page = Array.from({ length: pageSize }, (_, index) =>
      makeObserverEvent(pageSize - index),
    );
    const originalSort = Array.prototype.sort;
    let sortCalls = 0;
    Array.prototype.sort = function countedSort(compareFn) {
      sortCalls++;
      return originalSort.call(this, compareFn);
    };

    try {
      await ingestEvents(page);
    } finally {
      Array.prototype.sort = originalSort;
    }

    console.log(
      `observer_archive_ingest page_size=${pageSize} sort_calls=${sortCalls}`,
    );
    assert.equal(sortCalls, 1, "one page ingestion must perform one sort");
  });

  it("batched ingestion is byte-identical to repeated single-event ingestion", async () => {
    const events = Array.from({ length: 100 }, (_, index) =>
      makeObserverEvent(100 - index),
    );

    for (const event of events) {
      await ingestEvents([event]);
    }
    const singles = JSON.stringify(
      getArchivedChannelEvents(AGENT_PUBKEY, "channel-1"),
    );

    resetAgentObserverStore();
    _testRegisterKnownAgents(SUBSCRIPTION_ID, [AGENT_PUBKEY]);
    await ingestEvents(events);
    const batch = JSON.stringify(
      getArchivedChannelEvents(AGENT_PUBKEY, "channel-1"),
    );

    assert.equal(batch, singles);
  });
});

describe("idle observer agent retention", () => {
  beforeEach(() => {
    resetAgentObserverStore();
  });

  it("prunes idle unobserved data and replay rebuilds the transcript", () => {
    const activityAt = 1_000_000;
    const events = [
      makeObserverEvent(1, "channel-1"),
      makeObserverEvent(2, "channel-1", {
        kind: "turn_completed",
      }),
    ];
    const originalNow = Date.now;
    Date.now = () => activityAt;
    try {
      syncAgentObserverEvents(AGENT_PUBKEY, events);
    } finally {
      Date.now = originalNow;
    }
    const expectedTranscript = structuredClone(
      getAgentTranscript(AGENT_PUBKEY),
    );
    assert.equal(getAgentObserverSnapshot(AGENT_PUBKEY).events.length, 2);

    assert.equal(
      pruneIdleAgentObserverData([], activityAt + IDLE_RETENTION_MS - 1),
      false,
    );
    assert.equal(
      pruneIdleAgentObserverData([], activityAt + IDLE_RETENTION_MS),
      true,
    );
    assert.equal(getAgentObserverSnapshot(AGENT_PUBKEY).events.length, 0);
    assert.equal(getAgentTranscript(AGENT_PUBKEY).length, 0);

    syncAgentObserverEvents(AGENT_PUBKEY, events);
    assert.deepEqual(getAgentTranscript(AGENT_PUBKEY), expectedTranscript);
  });

  it("never prunes an agent with an active turn", () => {
    syncAgentObserverEvents(AGENT_PUBKEY, [makeObserverEvent(1, "channel-1")]);

    pruneIdleAgentObserverData(
      new Set([AGENT_PUBKEY]),
      Number.MAX_SAFE_INTEGER,
    );

    assert.equal(getAgentObserverSnapshot(AGENT_PUBKEY).events.length, 1);
  });

  it("retains a mounted agent projection until it unsubscribes", () => {
    syncAgentObserverEvents(AGENT_PUBKEY, [makeObserverEvent(1, "channel-1")]);
    const unsubscribe = subscribeAgentObserverProjection(
      AGENT_PUBKEY,
      () => {},
    );

    pruneIdleAgentObserverData([], Number.MAX_SAFE_INTEGER);
    assert.equal(getAgentObserverSnapshot(AGENT_PUBKEY).events.length, 1);

    unsubscribe();
    pruneIdleAgentObserverData([], Number.MAX_SAFE_INTEGER);
    assert.equal(getAgentObserverSnapshot(AGENT_PUBKEY).events.length, 0);
  });
});
