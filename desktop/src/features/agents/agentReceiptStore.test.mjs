import assert from "node:assert/strict";
import { beforeEach, describe, it } from "node:test";

import { deriveAgentConversationId } from "./conversationId.ts";
import {
  getAgentReceipts,
  getLatestAgentReceiptForConversation,
  ingestAgentReceiptEvent,
  ingestAgentReceiptReviewEvent,
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
    }),
    sig: "",
    ...overrides,
  };
}

describe("agentReceiptStore", () => {
  beforeEach(() => resetAgentReceiptStore());

  it("projects a durable receipt onto its conversation", () => {
    assert.equal(ingestAgentReceiptEvent(receiptEvent()), true);
    assert.deepEqual(getLatestAgentReceiptForConversation(CONVERSATION), {
      id: RECEIPT,
      channelId: CHANNEL,
      conversationId: CONVERSATION,
      agentPubkey: AGENT,
      createdAt: 100_000,
      summary: "Implemented recovery",
      verify: "pnpm check passed",
      reviewed: false,
    });
    assert.equal(getAgentReceipts().length, 1);
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
});
