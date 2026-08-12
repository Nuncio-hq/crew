/**
 * Issue #169 — Sleeping is a benign economy state on the Agents card.
 * Never Lost contact / Possibly stalled.
 */

import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const AGENT_PUBKEY = TEST_IDENTITIES.alice.pubkey;
const ACTIVE_RELAY_URL = (
  process.env.BUZZ_E2E_RELAY_URL ?? "http://localhost:3000"
).replace(/^http/, "ws");

test.describe("agent sleeping state (#169)", () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test("Sleeping badge shows on listening runtime and never Lost contact", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: "Sleepy",
          status: "running",
          channelNames: ["general"],
        },
      ],
      managedAgentRuntimes: [
        {
          pubkey: AGENT_PUBKEY,
          relayUrl: ACTIVE_RELAY_URL,
          lifecycle: "listening",
        },
      ],
      searchProfiles: [
        {
          pubkey: AGENT_PUBKEY,
          displayName: "Sleepy",
          ownerPubkey: "deadbeef".repeat(8),
          isAgent: true,
        },
      ],
    });

    await page.goto("/");
    await page.getByTestId("open-agents-view").click();
    await expect(page.getByTestId("unified-agents-groups")).toBeVisible({
      timeout: 10_000,
    });

    const card = page.getByTestId(`managed-agent-${AGENT_PUBKEY}`);
    await expect(card).toBeVisible();
    const badge = page.getByTestId(`agent-runtime-sleeping-${AGENT_PUBKEY}`);
    await expect(badge).toBeVisible();
    await expect(badge).toContainText("Sleeping · wakes on mention");
    await expect(page.getByText("Lost contact")).toHaveCount(0);
    await expect(page.getByText("Possibly stalled")).toHaveCount(0);

    await waitForAnimations(page);
    await card.screenshot({
      path: "test-results/screenshots-sleeping/01-sleeping-card.png",
    });
  });
});
