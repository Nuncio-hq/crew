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
  // Reject opens the reply composer (`onReply`), which mounts the same message
  // as the thread-head card. Scope to the timeline card so the dual render
  // (intentional product behavior) does not trip strict-mode uniqueness.
  const card = page
    .getByTestId("message-timeline")
    .getByTestId("evidence-card-test-run");
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

test("crew-evidence plus a buzz:// permalink still renders the evidence card", async ({
  page,
}) => {
  await installMockBridge(
    page,
    {
      searchProfiles: [
        {
          pubkey: AGENT_PUBKEY,
          displayName: "Evidence Agent",
          ownerPubkey: OWNER_PUBKEY,
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

  const prId = "ab".repeat(32);
  await page.evaluate(
    ({ pubkey, tags, prId, owner }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: `Evidence requiring review\nbuzz://pr?id=${prId}&owner=${owner}&d=crew`,
        pubkey,
        extraTags: tags,
      }),
    {
      pubkey: AGENT_PUBKEY,
      tags: EVIDENCE_TAG,
      prId,
      owner: OWNER_PUBKEY,
    },
  );

  const timeline = page.getByTestId("message-timeline");
  const card = timeline.getByTestId("evidence-card-test-run");
  await expect(card).toBeVisible();
  await expect(card.getByTestId("evidence-accept")).toBeVisible();
  await expect(card.getByTestId("evidence-reject")).toBeVisible();
  // Dispatch order: evidence card owns the row. A compact permalink chip
  // must not replace the body (that would look like a broken chip).
  const row = timeline.getByTestId("message-row").filter({
    has: page.getByTestId("evidence-card-test-run"),
  });
  await expect(row).toBeVisible();
  await expect(row.locator("[data-buzz-link-kind='pr']")).toHaveCount(0);
});

test("ordinary buzz:// permalinks render compact chips, not raw URLs", async ({
  page,
}) => {
  await installMockBridge(page);
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );
  const prId = "cd".repeat(32);
  const owner = "deadbeef".repeat(8);
  await page.evaluate(
    ({ prId, owner }) => {
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: [
          `buzz://pr?id=${prId}&owner=${owner}&d=relay-tools`,
          "buzz://channel/9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        ].join("\n"),
      });
    },
    { prId, owner },
  );

  const row = page.getByTestId("message-row").last();
  await expect(row.locator("[data-buzz-link-kind='pr']")).toBeVisible();
  await expect(row.locator("[data-channel-deep-link]")).toBeVisible();
  await expect(row).not.toContainText(`buzz://pr?id=${prId}`);
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
