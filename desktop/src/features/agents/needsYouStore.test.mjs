import assert from "node:assert/strict";
import { beforeEach, describe, it } from "node:test";

import {
  getNeedsYouForChannels,
  getNeedsYouForConversation,
  getNeedsYouForAll,
  getNeedsYouForChannel,
  ingestUserInputRequest,
  ingestApprovalRequest,
  ingestApprovalRequestEvent,
  reconcileNeedsYouFromFeed,
  resetNeedsYouStore,
  resolveApprovalRequest,
  resolveUserInputRequest,
  resolveApprovalRequestEvent,
  subscribeNeedsYou,
} from "./needsYouStore.ts";
import {
  KIND_APPROVAL_DENY,
  KIND_APPROVAL_GRANT,
  KIND_APPROVAL_REQUEST,
} from "../../shared/constants/kinds.ts";

const CHANNEL = "00112233-4455-6677-8899-aabbccddeeff";
const ROOT = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const REQUEST = "request-1";
const AGENT =
  "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const TOKEN_HASH = "b".repeat(64);

function request(overrides = {}) {
  return {
    id: REQUEST,
    channelId: CHANNEL,
    rootEventId: ROOT,
    agentPubkey: AGENT,
    createdAt: Date.now(),
    ...overrides,
  };
}

function event(overrides = {}) {
  return {
    id: ROOT,
    kind: KIND_APPROVAL_REQUEST,
    pubkey: AGENT,
    created_at: Math.floor(Date.now() / 1_000),
    tags: [
      ["h", CHANNEL],
      ["d", TOKEN_HASH],
    ],
    content: "",
    sig: "",
    ...overrides,
  };
}

describe("needsYouStore", () => {
  beforeEach(() => resetNeedsYouStore());

  it("maps an approval request to its conversation and channel", () => {
    const entry = ingestApprovalRequest(request());
    assert.equal(entry.conversationId, "7415ce56-7adc-d430-f133-c5e06a8e5113");
    assert.equal(getNeedsYouForConversation(entry.conversationId).length, 1);
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 1);
  });

  it("ingests root-only approval requests and clears on a d-tag grant", async () => {
    ingestApprovalRequestEvent(event());
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 1);
    assert.equal(
      await resolveApprovalRequestEvent(
        event({
          id: "grant",
          kind: KIND_APPROVAL_GRANT,
          tags: [["d", TOKEN_HASH]],
        }),
      ),
      true,
    );
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 0);
  });

  it("does not resurrect an approval when its terminal event arrives first", async () => {
    assert.equal(
      await resolveApprovalRequestEvent(
        event({
          id: "grant-first",
          kind: KIND_APPROVAL_GRANT,
          tags: [["d", TOKEN_HASH]],
        }),
      ),
      false,
    );
    ingestApprovalRequestEvent(event());
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 0);
  });

  it("fails closed on terminal overflow until a complete feed rebuild", async () => {
    for (let index = 0; index <= 1_000; index += 1) {
      await resolveApprovalRequestEvent(
        event({
          id: `terminal-${index}`,
          kind: KIND_APPROVAL_GRANT,
          tags: [["e", `request-${index}`, "", "reply"]],
        }),
      );
    }
    assert.equal(ingestApprovalRequestEvent(event({ id: "request-0" })), null);
    reconcileNeedsYouFromFeed([
      {
        id: "request-0",
        kind: KIND_APPROVAL_REQUEST,
        pubkey: "agent",
        content: "approval",
        createdAt: Date.now(),
        channelId: CHANNEL,
        channelName: "general",
        tags: [
          ["h", CHANNEL],
          ["e", ROOT, "", "root"],
        ],
        category: "needs_action",
      },
    ]);
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 1);
  });

  it("correlates a t-tag deny with the request token", async () => {
    ingestApprovalRequestEvent(event());
    assert.equal(
      await resolveApprovalRequestEvent(
        event({
          id: "deny",
          kind: KIND_APPROVAL_DENY,
          tags: [["t", TOKEN_HASH]],
        }),
      ),
      true,
    );
  });

  it("clears a request when the grant carries the raw token (desktop t-tag)", async () => {
    // Requests reference sha256(token); the desktop grant_approval command
    // publishes the RAW token in a `t` tag (src-tauri events.rs). The store
    // hashes grant references before giving up.
    const rawToken = "approval-token-uuid";
    const digest = await globalThis.crypto.subtle.digest(
      "SHA-256",
      new TextEncoder().encode(rawToken),
    );
    const tokenHash = [...new Uint8Array(digest)]
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");
    ingestApprovalRequestEvent(
      event({
        tags: [
          ["h", CHANNEL],
          ["d", tokenHash],
        ],
      }),
    );
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 1);
    assert.equal(
      await resolveApprovalRequestEvent(
        event({
          id: "grant-raw",
          kind: KIND_APPROVAL_GRANT,
          tags: [["t", rawToken]],
        }),
      ),
      true,
    );
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 0);
  });

  it("reconciles hydration: drops stale entries missing from the feed snapshot", () => {
    const now = Date.now();
    ingestApprovalRequest(
      request({ id: "stale-request", createdAt: now - 5 * 60 * 1_000 }),
    );
    ingestApprovalRequest(
      request({ id: "fresh-request", createdAt: now - 5_000 }),
    );
    // Feed snapshot contains neither → stale (past grace) drops, fresh survives.
    reconcileNeedsYouFromFeed([], now);
    const remaining = getNeedsYouForChannel(CHANNEL);
    assert.deepEqual(
      remaining.map((entry) => entry.id),
      ["fresh-request"],
    );
  });

  it("skips reconcile deletions when the feed snapshot may be partial", () => {
    const now = Date.now();
    ingestApprovalRequest(
      request({ id: "old-but-pending", createdAt: now - 10 * 60 * 1_000 }),
    );
    reconcileNeedsYouFromFeed([], now, { snapshotComplete: false });
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 1);
  });

  it("does not resurrect a live-resolved request from a stale feed page", async () => {
    ingestApprovalRequestEvent(event());
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 1);
    assert.equal(
      await resolveApprovalRequestEvent(
        event({
          id: "grant",
          kind: KIND_APPROVAL_GRANT,
          tags: [["d", TOKEN_HASH]],
        }),
      ),
      true,
    );
    // A feed page fetched before the grant landed still lists the request.
    const resurrection = ingestApprovalRequest(request({ id: ROOT }));
    assert.equal(resurrection, null);
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 0);
  });

  it("uses an unmarked e tag as the thread root", () => {
    const entry = ingestApprovalRequestEvent(
      event({
        id: "request-with-e",
        tags: [
          ["h", CHANNEL],
          ["e", ROOT],
        ],
      }),
    );
    assert.equal(entry?.rootEventId, ROOT);
    assert.equal(getNeedsYouForConversation(entry.conversationId).length, 1);
  });

  it("returns stable snapshots for unchanged state, including empty ids", () => {
    const emptyA = getNeedsYouForChannel(null);
    const emptyB = getNeedsYouForChannel(null);
    assert.strictEqual(emptyA, emptyB);
    const first = getNeedsYouForChannel(CHANNEL);
    const second = getNeedsYouForChannel(CHANNEL);
    assert.strictEqual(first, second);
  });

  it("invalidates the aggregate snapshot when content changes at the same count", () => {
    ingestApprovalRequest(request({ id: "second-request" }));
    const first = getNeedsYouForChannels([CHANNEL]);
    resolveApprovalRequest(REQUEST);
    ingestApprovalRequest(request({ id: "replacement-request" }));
    const second = getNeedsYouForChannels([CHANNEL]);
    assert.notStrictEqual(second, first);
    assert.deepEqual(
      second.map((entry) => entry.id),
      ["second-request", "replacement-request"],
    );
  });

  it("returns one stable all-channel snapshot for both request families", () => {
    ingestApprovalRequest(request({ id: "approval-all" }));
    ingestUserInputRequest({
      id: "user-input-all",
      channelId: CHANNEL,
      rootEventId: ROOT,
      conversationId: "conversation-user-input",
      agentPubkey: AGENT,
      createdAt: Date.now() + 1,
    });
    const first = getNeedsYouForAll();
    assert.deepEqual(
      first.map((entry) => entry.id),
      ["approval-all", "user-input-all"],
    );
    assert.strictEqual(first, getNeedsYouForAll());
    resolveUserInputRequest("user-input-all");
    assert.notStrictEqual(first, getNeedsYouForAll());
  });

  it("expires stale requests without notifying during a snapshot read", () => {
    const now = Date.now();
    ingestApprovalRequest(request({ createdAt: now - 1_000 }));
    let notifications = 0;
    const unsubscribe = subscribeNeedsYou(() => {
      notifications += 1;
    });
    notifications = 0;
    assert.equal(
      getNeedsYouForChannel(CHANNEL, now + 24 * 60 * 60 * 1_000).length,
      0,
    );
    assert.equal(notifications, 0);
    unsubscribe();
  });

  it("resolves a user-input request when an answer arrives", () => {
    ingestUserInputRequest({
      id: "user-input-answer",
      channelId: CHANNEL,
      rootEventId: ROOT,
      conversationId: "conversation-user-input",
      agentPubkey: AGENT,
      createdAt: Date.now(),
    });
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 1);
    resolveUserInputRequest("user-input-answer");
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 0);
  });

  it("resolves a user-input request on terminal resolution", () => {
    ingestUserInputRequest({
      id: "user-input-resolved",
      channelId: CHANNEL,
      rootEventId: ROOT,
      conversationId: "conversation-user-input",
      agentPubkey: AGENT,
      createdAt: Date.now(),
    });
    resolveUserInputRequest("user-input-resolved");
    assert.equal(
      getNeedsYouForConversation("conversation-user-input").length,
      0,
    );
  });

  it("uses a tombstone to block stale user-input re-ingestion", () => {
    const input = {
      id: "user-input-tombstone",
      channelId: CHANNEL,
      rootEventId: ROOT,
      conversationId: "conversation-user-input",
      agentPubkey: AGENT,
      createdAt: Date.now(),
    };
    ingestUserInputRequest(input);
    resolveUserInputRequest(input.id);
    assert.equal(ingestUserInputRequest(input), null);
    assert.equal(getNeedsYouForChannel(CHANNEL).length, 0);
  });

  it("retains durable user-input requests until resolution", () => {
    const now = Date.now();
    ingestUserInputRequest({
      id: "user-input-expired",
      channelId: CHANNEL,
      rootEventId: ROOT,
      conversationId: "conversation-user-input",
      agentPubkey: AGENT,
      createdAt: now - 30 * 60 * 1_000,
    });
    assert.equal(getNeedsYouForChannel(CHANNEL, now).length, 1);
  });

  it("reconcile never prunes user-input entries (46010-only feed)", () => {
    const now = Date.now();
    ingestUserInputRequest({
      id: "user-input-live",
      channelId: CHANNEL,
      rootEventId: ROOT,
      conversationId: "conversation-user-input",
      agentPubkey: AGENT,
      createdAt: now - 5 * 60 * 1_000, // well past the 60s grace
    });
    // The native needs_action feed only carries 46010 approvals, so a
    // complete snapshot without this entry must NOT delete it.
    reconcileNeedsYouFromFeed([], now);
    assert.equal(getNeedsYouForChannel(CHANNEL, now).length, 1);
    // Reset to isolate the module-level durable projection from later tests.
    resetNeedsYouStore();
  });
});
