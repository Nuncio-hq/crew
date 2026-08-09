import assert from "node:assert/strict";
import test from "node:test";

import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { AgentReceiptMessageBody } from "./AgentReceiptMessageBody.tsx";

const AGENT = "a".repeat(64);
const OWNER = "b".repeat(64);
const OTHER_USER = "c".repeat(64);
const message = {
  body: JSON.stringify({
    summary: "Recovery completed",
    verify: "pnpm check passed",
    lights: [],
    engineering: {},
  }),
  pubkey: AGENT,
};

function renderFor(currentPubkey, ownerPubkey) {
  return renderToStaticMarkup(
    React.createElement(AgentReceiptMessageBody, {
      canToggleReactions: true,
      currentPubkey,
      fallback: null,
      message,
      onReviewed() {},
      profiles: {
        [AGENT]: { ownerPubkey },
      },
      reactionPending: false,
      reactions: [
        {
          emoji: "✅",
          count: 1,
          reactedByCurrentUser: true,
          users: [],
        },
      ],
    }),
  );
}

test("owner can explicitly review an agent receipt", () => {
  const html = renderFor(OWNER, OWNER);
  assert.match(html, /data-testid="agent-receipt-reviewed"/);
  assert.match(html, />Reviewed</);
});

test("non-owner cannot review or claim an agent receipt as reviewed", () => {
  const html = renderFor(OTHER_USER, OWNER);
  assert.doesNotMatch(html, /data-testid="agent-receipt-reviewed"/);
  assert.doesNotMatch(html, />Reviewed</);
});
