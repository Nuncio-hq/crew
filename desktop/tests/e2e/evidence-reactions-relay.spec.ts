import { expect, test } from "@playwright/test";
import { hexToBytes } from "@noble/hashes/utils.js";
import { finalizeEvent } from "nostr-tools/pure";

import { installBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { assertRelaySeeded } from "../helpers/seed";

const RELAY_HTTP_URL =
  process.env.BUZZ_E2E_RELAY_URL ?? "http://localhost:3000";
const GENERAL_CHANNEL_ID = "9f28288a-d724-587a-9709-92dc7f967110";
const EVIDENCE_TAG = [["crew-evidence", "test-run"]];

async function publishEvidence() {
  const event = finalizeEvent(
    {
      kind: 9,
      content: `Evidence requiring review ${Date.now()}`,
      tags: [["h", GENERAL_CHANNEL_ID], ...EVIDENCE_TAG],
      created_at: Math.floor(Date.now() / 1000),
    },
    hexToBytes(TEST_IDENTITIES.alice.privateKey),
  );
  const response = await fetch(`${RELAY_HTTP_URL}/events`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Pubkey": event.pubkey,
    },
    body: JSON.stringify(event),
  });
  if (!response.ok) {
    throw new Error(
      `POST /events failed (${response.status}): ${await response.text()}`,
    );
  }
  return event;
}

async function queryReactions(eventId: string) {
  const response = await fetch(`${RELAY_HTTP_URL}/query`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Pubkey": TEST_IDENTITIES.tyler.pubkey,
    },
    body: JSON.stringify([
      {
        kinds: [7],
        "#e": [eventId],
      },
    ]),
  });
  if (!response.ok) {
    throw new Error(
      `POST /query failed (${response.status}): ${await response.text()}`,
    );
  }
  return (await response.json()) as Array<{
    content: string;
    kind: number;
    pubkey: string;
    tags: string[][];
  }>;
}

async function expectReaction(eventId: string, content: string) {
  await expect
    .poll(
      async () =>
        (await queryReactions(eventId)).find(
          (event) =>
            event.pubkey === TEST_IDENTITIES.tyler.pubkey &&
            event.content === content &&
            event.kind === 7 &&
            event.tags.some(
              (tag) => tag.length === 2 && tag[0] === "e" && tag[1] === eventId,
            ),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();
}

test.beforeAll(async () => {
  await assertRelaySeeded();
});

test("owner Accept and Reject publish real relay reactions", async ({
  page,
}) => {
  await installBridge(page, {
    mode: "relay",
    user: "tyler",
    relayHttpUrl: RELAY_HTTP_URL,
    relayWsUrl: RELAY_HTTP_URL.replace(/^http/, "ws"),
    mock: {
      searchProfiles: [
        {
          pubkey: TEST_IDENTITIES.alice.pubkey,
          displayName: "Evidence Agent",
          ownerPubkey: TEST_IDENTITIES.tyler.pubkey,
          isAgent: true,
        },
      ],
    },
  });
  const evidence = await publishEvidence();

  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  const card = page
    .getByTestId("evidence-card-test-run")
    .filter({ hasText: evidence.content });
  await expect(card).toBeVisible();

  await card.getByTestId("evidence-accept").click();
  await expectReaction(evidence.id, "✅");
  await expect(card.getByTestId("evidence-reaction-accepted")).toBeVisible();

  await card.getByTestId("evidence-reject").click();
  await expectReaction(evidence.id, "❌");
  await expect(page.getByTestId("message-composer").last()).toBeVisible();
  await expect(card.getByTestId("evidence-reaction-rejected")).toBeVisible();
});
