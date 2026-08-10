import { expect, test } from "@playwright/test";

import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const EVIDENCE_TAG = [["crew-evidence", "test-run"]];
const AGENT_PUBKEY = TEST_IDENTITIES.alice.pubkey;
const OWNER_PUBKEY = "deadbeef".repeat(8);

async function openEvidence(
  page: import("@playwright/test").Page,
  ownerPubkey = OWNER_PUBKEY,
  agentPubkey = AGENT_PUBKEY,
) {
  await installMockBridge(
    page,
    {
      searchProfiles: [
        {
          pubkey: agentPubkey,
          displayName: "Evidence Agent",
          ownerPubkey,
          isAgent: true,
        },
      ],
    },
    { autoConnectDefaultRelay: true },
  );
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );
  return page.evaluate(
    ({ pubkey, tags }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: "Evidence requiring review",
        pubkey,
        extraTags: tags,
      }),
    { pubkey: agentPubkey, tags: EVIDENCE_TAG },
  );
}

test("owner Accept and Reject round-trip as reactions on the evidence card", async ({
  page,
}) => {
  const evidence = await openEvidence(page);
  const card = page.getByTestId("evidence-card-test-run");
  await expect(card).toBeVisible();

  await page.evaluate(
    ({ eventId, pubkey }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: "❌",
        kind: 7,
        pubkey,
        extraTags: [["e", eventId]],
      }),
    { eventId: evidence.id, pubkey: TEST_IDENTITIES.bob.pubkey },
  );
  await expect(card.getByTestId("evidence-reaction-rejected")).toHaveCount(0);

  await card.getByTestId("evidence-accept").click();
  await expect(card.getByTestId("evidence-reaction-accepted")).toBeVisible();

  await card.getByTestId("evidence-reject").click();
  await expect(page.getByTestId("message-composer").last()).toBeVisible();
  await expect(card.getByTestId("evidence-reaction-rejected")).toBeVisible();
});

test("non-owner sees evidence card without review controls", async ({
  page,
}) => {
  await openEvidence(
    page,
    TEST_IDENTITIES.alice.pubkey,
    TEST_IDENTITIES.bob.pubkey,
  );
  const card = page.getByTestId("evidence-card-test-run");
  await expect(card).toBeVisible();
  await expect(card.getByTestId("evidence-accept")).toHaveCount(0);
  await expect(card.getByTestId("evidence-reject")).toHaveCount(0);
});

test("reactions on an ordinary message remain available", async ({ page }) => {
  await installMockBridge(page);
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );
  await page.evaluate(() => {
    window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "general",
      content: "ordinary reaction target",
    });
  });
  const row = page
    .getByTestId("message-row")
    .filter({ hasText: "ordinary reaction target" });
  await row.hover();
  await row.getByRole("button", { name: "React with :+1:" }).click();
  await expect(row.getByLabel("Toggle 👍 reaction")).toBeVisible();
});
