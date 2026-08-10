import assert from "node:assert/strict";
import { beforeEach, describe, it } from "node:test";

import { deriveAgentConversationId } from "./conversationId.ts";
import {
  _testPendingAgentReceiptReviewCount,
  beginExhaustiveAgentReceiptProjection,
  endExhaustiveAgentReceiptProjection,
  getAgentReceipts,
  getLatestAgentReceiptForConversation,
  getLatestOwnedAgentReceiptForActiveTurns,
  getLatestOwnedAgentReceiptForConversation,
  ingestAgentReceiptEvent as ingestValidatedAgentReceiptEvent,
  ingestAgentReceiptReviewEvent as ingestValidatedAgentReceiptReviewEvent,
  markAgentReceiptProjectionUnavailable,
  resetAgentReceiptStore,
} from "./agentReceiptStore.ts";

const CHANNEL = "94a444a4-c0a3-5966-ab05-530c6ddc2301";
const ROOT = "a".repeat(64);
const RECEIPT = "b".repeat(64);
const AGENT = "c".repeat(64);
const OWNER = "d".repeat(64);
const OTHER_AGENT = "8".repeat(64);
const CONVERSATION = deriveAgentConversationId(CHANNEL, ROOT);
const OWNED_AGENTS = new Set([AGENT]);
let projectionOwner;

function receiptEvent(overrides = {}) {
  return {
    id: RECEIPT,
    pubkey: AGENT,
    created_at: 100,
    kind: 46043,
    tags: [
      ["h", CHANNEL],
      ["e", ROOT, "", "root"],
      ["e", ROOT, "", "reply"],
    ],
    content: JSON.stringify({
      summary: "Implemented recovery",
      verify: "pnpm check passed",
      lights: [{ label: "Desktop", status: "passed" }],
      engineering: {},
      run: { session_id: "session", turn_id: "turn" },
    }),
    sig: "",
    ...overrides,
  };
}

function ingestAgentReceiptEvent(event) {
  const parent = {
    id: ROOT,
    pubkey: OWNER,
    created_at: 99,
    kind: 9,
    tags: [
      ["h", CHANNEL],
      ["p", event.pubkey],
    ],
    content: "trigger",
    sig: "",
  };
  return ingestValidatedAgentReceiptEvent(
    event,
    parent,
    new Map([[parent.id, parent]]),
    projectionOwner,
  );
}

function ingestAgentReceiptReviewEvent(
  event,
  currentPubkey,
  ownedAgentPubkeys,
) {
  return ingestValidatedAgentReceiptReviewEvent(
    event,
    currentPubkey,
    ownedAgentPubkeys,
    projectionOwner,
  );
}

describe("agentReceiptStore", () => {
  beforeEach(() => {
    resetAgentReceiptStore();
    projectionOwner = beginExhaustiveAgentReceiptProjection(
      OWNER,
      OWNED_AGENTS,
    );
    endExhaustiveAgentReceiptProjection(projectionOwner);
  });

  it("projects a durable receipt onto its conversation", () => {
    assert.equal(ingestAgentReceiptEvent(receiptEvent()), true);
    assert.deepEqual(getLatestAgentReceiptForConversation(CONVERSATION), {
      id: RECEIPT,
      channelId: CHANNEL,
      conversationId: deriveAgentConversationId(CHANNEL, ROOT),
      rootEventId: ROOT,
      parentEventId: ROOT,
      agentPubkey: AGENT,
      sessionId: "session",
      turnId: "turn",
      createdAt: 100_000,
      summary: "Implemented recovery",
      verify: "pnpm check passed",
      reviewed: false,
    });
    assert.equal(getAgentReceipts().length, 1);
  });

  it("rejects a receipt whose declared root does not match its parent", () => {
    assert.equal(
      ingestAgentReceiptEvent(
        receiptEvent({
          tags: [
            ["h", CHANNEL],
            ["e", "f".repeat(64), "", "root"],
            ["e", ROOT, "", "reply"],
          ],
        }),
      ),
      false,
    );
  });

  it("rejects legacy receipts that cannot prove their producer run", () => {
    assert.equal(
      ingestAgentReceiptEvent(
        receiptEvent({
          content: JSON.stringify({
            summary: "Legacy completion",
            verify: "manual",
            lights: [],
            engineering: {},
          }),
        }),
      ),
      false,
    );
  });

  it("rejects a receipt when its parent has no canonical agent target", () => {
    const event = receiptEvent();
    const parent = {
      id: ROOT,
      pubkey: OWNER,
      created_at: 99,
      kind: 9,
      tags: [["h", CHANNEL]],
      content: "trigger",
      sig: "",
    };
    assert.equal(ingestValidatedAgentReceiptEvent(event, parent), false);
    assert.equal(
      ingestValidatedAgentReceiptEvent(event, {
        ...parent,
        tags: [["h", CHANNEL], ["p"]],
      }),
      false,
    );
  });

  it("marks reviewed only from the owner's explicit check reaction", () => {
    ingestAgentReceiptEvent(receiptEvent());
    assert.equal(
      ingestAgentReceiptReviewEvent(
        {
          id: "e".repeat(64),
          pubkey: OWNER,
          created_at: 101,
          kind: 7,
          tags: [["e", RECEIPT]],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      ),
      true,
    );
    assert.equal(
      getLatestAgentReceiptForConversation(CONVERSATION).reviewed,
      true,
    );
  });

  it("applies an authorized review that arrives before its receipt", () => {
    assert.equal(
      ingestAgentReceiptReviewEvent(
        {
          id: "1".repeat(64),
          pubkey: OWNER,
          created_at: 101,
          kind: 7,
          tags: [["e", RECEIPT]],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      ),
      false,
    );

    assert.equal(ingestAgentReceiptEvent(receiptEvent()), true);
    assert.equal(
      getLatestAgentReceiptForConversation(CONVERSATION)?.reviewed,
      true,
    );
  });

  it("does not evict an authorized review before its receipt arrives", () => {
    ingestAgentReceiptReviewEvent(
      {
        id: "1".repeat(64),
        pubkey: OWNER,
        created_at: 101,
        kind: 7,
        tags: [["e", RECEIPT]],
        content: "✅",
        sig: "",
      },
      OWNER,
      OWNED_AGENTS,
    );
    for (let index = 1; index <= 500; index += 1) {
      ingestAgentReceiptReviewEvent(
        {
          id: index.toString(16).padStart(64, "0"),
          pubkey: OWNER,
          created_at: 101 + index,
          kind: 7,
          tags: [["e", (index + 1).toString(16).padStart(64, "0")]],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      );
    }

    assert.equal(ingestAgentReceiptEvent(receiptEvent()), true);
    assert.equal(
      getLatestAgentReceiptForConversation(CONVERSATION)?.reviewed,
      true,
    );
  });

  it("does not discard an authorized review beyond the old candidate cap", () => {
    const targetReceiptId = (2_512).toString(16).padStart(64, "0");
    for (let index = 0; index < 513; index += 1) {
      ingestAgentReceiptReviewEvent(
        {
          id: (index + 1).toString(16).padStart(64, "0"),
          pubkey: OWNER,
          created_at: 101 + index,
          kind: 7,
          tags: [["e", (index + 2_000).toString(16).padStart(64, "0")]],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      );
    }

    assert.equal(_testPendingAgentReceiptReviewCount(), 513);
    assert.equal(
      ingestAgentReceiptEvent(receiptEvent({ id: targetReceiptId })),
      true,
    );
    assert.equal(
      getAgentReceipts().find((receipt) => receipt.id === targetReceiptId)
        ?.reviewed,
      true,
    );
  });

  it("rejects malformed receipts and another user's reactions", () => {
    assert.equal(
      ingestAgentReceiptEvent(receiptEvent({ content: "not-json" })),
      false,
    );
    ingestAgentReceiptEvent(receiptEvent());
    assert.equal(
      ingestAgentReceiptReviewEvent(
        {
          id: "f".repeat(64),
          pubkey: "9".repeat(64),
          created_at: 101,
          kind: 7,
          tags: [["e", RECEIPT]],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      ),
      false,
    );
  });

  it("rejects a current-user reaction when that user does not own the receipt agent", () => {
    ingestAgentReceiptEvent(receiptEvent({ pubkey: OTHER_AGENT }));
    assert.equal(
      ingestAgentReceiptReviewEvent(
        {
          id: "7".repeat(64),
          pubkey: OWNER,
          created_at: 101,
          kind: 7,
          tags: [["e", RECEIPT]],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      ),
      false,
    );
    assert.equal(
      getLatestAgentReceiptForConversation(CONVERSATION).reviewed,
      false,
    );
  });

  it("does not let a newer foreign receipt shadow the owner's thread status", () => {
    ingestAgentReceiptEvent(receiptEvent());
    ingestAgentReceiptEvent(
      receiptEvent({
        id: "9".repeat(64),
        pubkey: OTHER_AGENT,
        created_at: 200,
      }),
    );

    assert.equal(
      getLatestOwnedAgentReceiptForConversation(CONVERSATION, OWNED_AGENTS)?.id,
      RECEIPT,
    );
  });

  it("hides conversation receipts after active turns complete", () => {
    const lower = "1".repeat(64);
    const higher = "f".repeat(64);
    ingestAgentReceiptEvent(receiptEvent({ id: lower }));
    ingestAgentReceiptEvent(receiptEvent({ id: higher }));
    const active = [
      {
        agentPubkey: AGENT,
        runs: [
          {
            sessionId: "session",
            turnId: "turn",
            triggeringEventIds: [ROOT],
          },
        ],
      },
    ];
    assert.equal(
      getLatestOwnedAgentReceiptForActiveTurns(
        CONVERSATION,
        OWNED_AGENTS,
        active,
      )?.id,
      higher,
    );
    assert.equal(
      getLatestOwnedAgentReceiptForActiveTurns(CONVERSATION, OWNED_AGENTS, []),
      null,
    );
  });

  it("uses the last valid e tag as the NIP-25 direct target", () => {
    ingestAgentReceiptEvent(receiptEvent());
    assert.equal(
      ingestAgentReceiptReviewEvent(
        {
          id: "6".repeat(64),
          pubkey: OWNER,
          created_at: 101,
          kind: 7,
          tags: [
            ["e", RECEIPT],
            ["e", "5".repeat(64)],
          ],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      ),
      false,
    );
    assert.equal(
      getLatestAgentReceiptForConversation(CONVERSATION).reviewed,
      false,
    );
  });

  it("skips a malformed trailing e tag when finding the direct target", () => {
    ingestAgentReceiptEvent(receiptEvent());
    assert.equal(
      ingestAgentReceiptReviewEvent(
        {
          id: "4".repeat(64),
          pubkey: OWNER,
          created_at: 101,
          kind: 7,
          tags: [
            ["e", RECEIPT],
            ["e", "not-an-event-id"],
          ],
          content: "✅",
          sig: "",
        },
        OWNER,
        OWNED_AGENTS,
      ),
      true,
    );
  });

  it("rejects stale exhaustive receipt projection owners", () => {
    const older = beginExhaustiveAgentReceiptProjection(OWNER, OWNED_AGENTS);
    const newer = beginExhaustiveAgentReceiptProjection(OWNER, OWNED_AGENTS);
    assert.equal(endExhaustiveAgentReceiptProjection(older), false);
    assert.equal(markAgentReceiptProjectionUnavailable(older), false);
    assert.equal(endExhaustiveAgentReceiptProjection(newer), true);
  });
});
