import assert from "node:assert/strict";
import { after, afterEach, test } from "node:test";

import { JSDOM } from "jsdom";
import * as React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";

import {
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";
import {
  beginExhaustiveAgentReceiptProjection,
  ingestAgentReceiptEvent,
  resetAgentReceiptStore,
} from "./agentReceiptStore.ts";
import { deriveAgentConversationId } from "./conversationId.ts";
import {
  resetAgentObserverStore,
  setObserverConnectionStateForE2E,
} from "./observerRelayStore.ts";

const CHANNEL = "94a444a4-c0a3-5966-ab05-530c6ddc2301";
const ROOT = "a".repeat(64);
const AGENT = "b".repeat(64);
const OWNER = "c".repeat(64);
const CONVERSATION = deriveAgentConversationId(CHANNEL, ROOT);

const dom = new JSDOM(
  "<!doctype html><html><body><div id='root'></div></body></html>",
  { url: "http://localhost" },
);
Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});
if (!window.matchMedia) {
  window.matchMedia = () => ({
    addEventListener() {},
    addListener() {},
    matches: false,
    media: "",
    removeEventListener() {},
    removeListener() {},
  });
}

const { ThreadAgentStatusChip } = await import(
  "../messages/ui/ThreadAgentStatusChip.tsx"
);

function turnStarted() {
  return {
    agentIndex: 0,
    channelId: CHANNEL,
    conversationId: CONVERSATION,
    kind: "turn_started",
    payload: { triggeringEventIds: [ROOT] },
    replayed: false,
    seq: 1,
    sessionId: "session-1",
    timestamp: "2026-09-15T00:00:00.000Z",
    turnId: "turn-1",
  };
}

const parentEvent = {
  content: "Do the work",
  created_at: 100,
  id: ROOT,
  kind: 9,
  pubkey: OWNER,
  sig: "",
  tags: [
    ["h", CHANNEL],
    ["p", AGENT],
  ],
};

const receiptEvent = {
  content: JSON.stringify({
    engineering: {},
    lights: [{ label: "Check", status: "passed" }],
    run: { session_id: "session-1", turn_id: "turn-1" },
    summary: "Work ready for review",
    verify: "Checks passed",
  }),
  created_at: Math.floor(Date.now() / 1000),
  id: "d".repeat(64),
  kind: 46043,
  pubkey: AGENT,
  sig: "",
  tags: [
    ["h", CHANNEL],
    ["e", ROOT, "", "root"],
    ["e", ROOT, "", "reply"],
  ],
};

const profiles = {
  [AGENT]: {
    avatarUrl: null,
    displayName: "Claude",
    isAgent: true,
    name: "claude",
    nip05Handle: null,
    ownerPubkey: OWNER,
  },
};

let root;

afterEach(async () => {
  if (root) {
    await act(async () => root.unmount());
    root = undefined;
  }
  resetActiveAgentTurnsStore();
  resetAgentReceiptStore();
  resetAgentObserverStore();
  document.getElementById("root").replaceChildren();
});

after(() => dom.window.close());

test("receipt arriving before turn completion keeps the mounted status chip stable", async () => {
  resetActiveAgentTurnsStore();
  resetAgentReceiptStore();
  resetAgentObserverStore();
  setObserverConnectionStateForE2E("open");
  const projectionOwner = beginExhaustiveAgentReceiptProjection(
    OWNER,
    new Set([AGENT]),
  );
  syncAgentTurnsFromEvents(AGENT, [turnStarted()]);

  root = createRoot(document.getElementById("root"));
  await act(async () =>
    root.render(
      React.createElement(ThreadAgentStatusChip, {
        conversationId: CONVERSATION,
        currentPubkey: OWNER,
        profiles,
      }),
    ),
  );

  const ancestry = new Map([[ROOT, parentEvent]]);
  await act(async () => {
    assert.equal(
      ingestAgentReceiptEvent(
        receiptEvent,
        parentEvent,
        ancestry,
        projectionOwner,
      ),
      true,
    );
  });

  assert.equal(
    document
      .querySelector("[data-testid='thread-agent-status-chip']")
      ?.getAttribute("data-state"),
    "ready-to-review",
  );
});
