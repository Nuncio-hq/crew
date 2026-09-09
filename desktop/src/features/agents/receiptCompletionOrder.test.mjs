import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";

import { buildThreadAgentStatusChipView } from "../messages/ui/ThreadAgentStatusChip.tsx";
import {
  getActiveTurnControlTargetsForAgent,
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";
import { getActiveTurnSummariesForConversation } from "./activeConversationAgentTurnSummaries.ts";
import {
  beginExhaustiveAgentReceiptProjection,
  getAgentReceipts,
  getLatestOwnedAgentReceiptForActiveTurns,
  ingestAgentReceiptEvent,
  resetAgentReceiptStore,
} from "./agentReceiptStore.ts";
import { deriveAgentConversationId } from "./conversationId.ts";

const CHANNEL = "94a444a4-c0a3-5966-ab05-530c6ddc2301";
const ROOT = "a".repeat(64);
const AGENT = "b".repeat(64);
const OWNER = "c".repeat(64);
const OWNED = new Set([AGENT]);
const CONVERSATION = deriveAgentConversationId(CHANNEL, ROOT);
let projectionOwner;

function observer(seq, kind, turnId = "turn-1") {
  return {
    seq,
    kind,
    timestamp: new Date().toISOString(),
    agentIndex: 0,
    channelId: CHANNEL,
    conversationId: CONVERSATION,
    sessionId: "session-1",
    turnId,
    payload: { triggeringEventIds: [ROOT] },
  };
}

const parent = {
  id: ROOT,
  pubkey: OWNER,
  kind: 9,
  tags: [
    ["h", CHANNEL],
    ["p", AGENT],
  ],
  content: "Do the work",
  created_at: 100,
  sig: "",
};

function receipt() {
  return {
    id: "d".repeat(64),
    pubkey: AGENT,
    kind: 46043,
    tags: [
      ["h", CHANNEL],
      ["e", ROOT, "", "root"],
      ["e", ROOT, "", "reply"],
    ],
    created_at: Math.floor(Date.now() / 1000),
    sig: "",
    content: JSON.stringify({
      summary: "Work ready for review",
      verify: "Checks passed",
      lights: [{ label: "Check", status: "passed" }],
      engineering: {},
      run: { session_id: "session-1", turn_id: "turn-1" },
    }),
  };
}

function view() {
  const summaries = getActiveTurnSummariesForConversation(CONVERSATION);
  const matched = getLatestOwnedAgentReceiptForActiveTurns(
    CONVERSATION,
    OWNED,
    summaries,
  );
  return buildThreadAgentStatusChipView(
    summaries,
    null,
    undefined,
    Date.now(),
    [],
    matched,
    "open",
  );
}

beforeEach(() => {
  resetActiveAgentTurnsStore();
  resetAgentReceiptStore();
  projectionOwner = beginExhaustiveAgentReceiptProjection(OWNER, OWNED);
});
afterEach(() => {
  resetActiveAgentTurnsStore();
  resetAgentReceiptStore();
});

test("receipt before observer completion settles the exact badge, not control authority or founder acceptance", () => {
  syncAgentTurnsFromEvents(AGENT, [observer(1, "turn_started")]);
  assert.equal(view()?.state, "running");
  const event = receipt();
  const ancestry = new Map([[ROOT, parent]]);
  assert.equal(
    ingestAgentReceiptEvent(event, parent, ancestry, projectionOwner),
    true,
  );
  assert.equal(view()?.state, "ready-to-review");
  assert.equal(
    getAgentReceipts()[0].reviewed,
    false,
    "receipt does not manufacture founder acceptance",
  );
  assert.equal(
    getActiveTurnControlTargetsForAgent(AGENT).length,
    1,
    "controls still follow observer lifecycle",
  );
  assert.equal(
    ingestAgentReceiptEvent(event, parent, ancestry, projectionOwner),
    false,
    "duplicate is idempotent",
  );

  syncAgentTurnsFromEvents(AGENT, [observer(2, "turn_completed")]);
  assert.deepEqual(getActiveTurnControlTargetsForAgent(AGENT), []);
  syncAgentTurnsFromEvents(AGENT, [observer(3, "turn_started", "turn-2")]);
  assert.equal(
    view()?.state,
    "running",
    "same-session T1 receipt must not settle T2",
  );
  assert.equal(
    ingestAgentReceiptEvent(event, parent, ancestry, projectionOwner),
    false,
  );
  syncAgentTurnsFromEvents(AGENT, [observer(4, "turn_completed")]);
  assert.equal(
    view()?.state,
    "running",
    "later-sequence T1 terminal must not remove T2",
  );
  assert.equal(getActiveTurnControlTargetsForAgent(AGENT)[0].turnId, "turn-2");
});
