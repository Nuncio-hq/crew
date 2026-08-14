import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const SHOTS = "test-results/org-hierarchy";
const OWNER = "deadbeef".repeat(8);
const HERMES = TEST_IDENTITIES.alice.pubkey;
const CODY = TEST_IDENTITIES.bob.pubkey;
const KICKOFF_ID = "1".repeat(64);
const STOP_ID = "3".repeat(64);
const PARENT_ID = "4".repeat(64);

function rosterContent() {
  return JSON.stringify({
    nodes: {
      [HERMES]: {
        manager: OWNER,
        domain: "office",
        duties: "survey the floor",
        cadence: "weekly",
        budget: { tokens_per_day: 100000, open_work_cap: 3 },
      },
      [CODY]: {
        manager: HERMES,
        domain: "execution",
        duties: "build",
        cadence: "daily",
        budget: { tokens_per_day: 40000, open_work_cap: 1 },
      },
    },
  });
}

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

test.describe("org hierarchy (#198)", () => {
  test("chart, editor, handoff chip, budget stop, and skip-level mention", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: HERMES,
          name: "Hermes",
          status: "running",
          channelNames: ["engineering"],
        },
        {
          pubkey: CODY,
          name: "Cody",
          status: "running",
          channelNames: ["engineering"],
        },
      ],
      searchProfiles: [
        {
          pubkey: HERMES,
          displayName: "Hermes",
          ownerPubkey: OWNER,
          isAgent: true,
        },
        {
          pubkey: CODY,
          displayName: "Cody",
          ownerPubkey: OWNER,
          isAgent: true,
        },
      ],
    });

    await page.goto("/");
    await page.evaluate(
      ({ content, pubkey }) => {
        window.__BUZZ_E2E_SET_ORG_ROSTER__?.({ content, pubkey });
      },
      { content: rosterContent(), pubkey: OWNER },
    );

    await page.getByTestId("open-org-view").click();
    await expect(page).toHaveURL(/#\/org$/);
    await expect(page.getByTestId("org-view")).toBeVisible();
    await expect(page.getByTestId(`org-node-${HERMES}`)).toBeVisible();
    await expect(page.getByTestId(`org-node-${CODY}`)).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("org-chart")
      .screenshot({ path: `${SHOTS}/01-org-chart.png` });

    await page.getByTestId(`org-node-${HERMES}`).click();
    await expect(page.getByTestId("org-drill-panel")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("org-drill-panel")
      .screenshot({ path: `${SHOTS}/02-org-drill.png` });

    await page.getByTestId("org-edit-roster").click();
    await expect(page.getByTestId("org-roster-editor")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("org-roster-editor")
      .screenshot({ path: `${SHOTS}/03-org-editor.png` });
    await page.keyboard.press("Escape");

    await page.getByTestId("channel-engineering").click();
    await expect
      .poll(async () =>
        page.evaluate(
          () =>
            window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
              channelName: "engineering",
            }) ?? false,
        ),
      )
      .toBe(true);
    await page.evaluate(
      ({ kickoffId, stopId, parentId, cody, hermes }) => {
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "engineering",
          content: "Ship the office loop",
          id: kickoffId,
          extraTags: [
            ["crew-handoff", cody, "a".repeat(64)],
            ["e", parentId, "", "root"],
          ],
        });
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "engineering",
          content: "⛔ Budget reached · 3 items queued",
          id: stopId,
          pubkey: hermes,
          extraTags: [["crew-budget", "stop"]],
        });
      },
      {
        kickoffId: KICKOFF_ID,
        stopId: STOP_ID,
        parentId: PARENT_ID,
        cody: CODY,
        hermes: HERMES,
      },
    );

    await expect(page.getByTestId(`handoff-chip-${KICKOFF_ID}`)).toBeVisible();
    await expect(page.getByTestId(`budget-stop-${STOP_ID}`)).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId(`handoff-chip-${KICKOFF_ID}`)
      .screenshot({ path: `${SHOTS}/04-handoff-chip.png` });
    await page
      .getByTestId(`budget-stop-${STOP_ID}`)
      .screenshot({ path: `${SHOTS}/05-budget-stop.png` });

    await page.getByTestId("message-input").fill("@Hermes skip-level ping");
    await expect(page.getByTestId("message-input")).toContainText("Hermes");
  });
});
