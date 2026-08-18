import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const SHOTS = "test-results/wiki-org-office-chrome";
const OWNER = "deadbeef".repeat(8);
const HERMES = TEST_IDENTITIES.alice.pubkey;

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

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
    },
  });
}

async function surfaceKind(
  locator: import("@playwright/test").Locator,
): Promise<string | null> {
  return locator.getAttribute("data-office-surface");
}

async function rgbOf(
  locator: import("@playwright/test").Locator,
  property: "backgroundColor" | "borderTopColor" | "color",
): Promise<string> {
  return locator.evaluate(
    (element, key) => getComputedStyle(element)[key],
    property,
  );
}

async function borderWidthPx(
  locator: import("@playwright/test").Locator,
): Promise<number> {
  return locator.evaluate((element) =>
    Number.parseFloat(getComputedStyle(element).borderTopWidth),
  );
}

test.describe("Wiki + Org office chrome and Wiki IA (#221)", () => {
  test("org roster fields, wiki home IA, and repo wiki ask are distinct surfaces", async ({
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
      ],
      searchProfiles: [
        {
          pubkey: HERMES,
          displayName: "Hermes",
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
    await expect(page.getByTestId("org-view")).toBeVisible();
    await expect(page.getByTestId("org-header-bar")).toHaveAttribute(
      "data-office-surface",
      "header-bar",
    );

    await page.getByTestId("org-edit-roster").click();
    const editor = page.getByTestId("org-roster-editor");
    await expect(editor).toBeVisible();

    const agentLabel = editor.getByText("Agent", { exact: true });
    await expect(agentLabel).toBeVisible();
    await expect(agentLabel.locator("select")).toHaveCount(0);
    await expect(agentLabel.locator("input")).toHaveCount(0);

    const agentField = editor
      .locator('[data-office-surface="field-box"]')
      .first();
    await expect(agentField).toBeVisible();
    expect(await borderWidthPx(agentField)).toBeGreaterThanOrEqual(1);
    expect(await rgbOf(agentField, "backgroundColor")).not.toBe(
      await rgbOf(editor, "backgroundColor"),
    );

    await waitForAnimations(page);
    await editor.screenshot({ path: `${SHOTS}/01-org-roster-editor.png` });
    await page.keyboard.press("Escape");

    await page.getByTestId("open-wiki-view").click();
    await expect(page.getByTestId("wiki-library")).toBeVisible();
    await expect(page.getByText("Create company page")).toHaveCount(0);
    await expect(page.getByTestId("wiki-company-title")).toHaveCount(0);
    await expect(page.getByTestId("wiki-company-body")).toHaveCount(0);
    await expect(page.getByTestId("wiki-home-search")).toBeVisible();
    await expect(
      page.getByRole("heading", {
        name: "Which repo would you like to understand?",
      }),
    ).toBeVisible();
    await expect(page.getByTestId("wiki-repo-card-buzz")).toBeVisible();
    await expect(page.getByTestId("wiki-header-bar")).toHaveAttribute(
      "data-office-surface",
      "header-bar",
    );
    expect(await surfaceKind(page.getByTestId("wiki-home-search"))).toBe(
      "field-box",
    );
    expect(
      await borderWidthPx(page.getByTestId("wiki-home-search")),
    ).toBeGreaterThanOrEqual(1);

    await waitForAnimations(page);
    await page
      .getByTestId("wiki-library")
      .screenshot({ path: `${SHOTS}/02-wiki-home.png` });

    await page
      .getByTestId("wiki-repo-card-buzz")
      .locator("button")
      .first()
      .click();
    await expect(page.getByTestId("wiki-page")).toBeVisible();
    await expect(page.getByTestId("wiki-header-bar")).toHaveAttribute(
      "data-office-surface",
      "header-bar",
    );
    await expect(
      page.getByTestId("wiki-header-bar").getByTestId("wiki-generate-mirror"),
    ).toBeVisible();
    await expect(page.getByTestId("wiki-ask")).toHaveAttribute(
      "data-office-surface",
      "composer-surface",
    );
    expect(
      await borderWidthPx(page.getByTestId("wiki-ask")),
    ).toBeGreaterThanOrEqual(1);
    expect(
      await rgbOf(page.getByTestId("wiki-ask"), "backgroundColor"),
    ).not.toBe(await rgbOf(page.getByTestId("wiki-page"), "backgroundColor"));

    await waitForAnimations(page);
    await page
      .getByTestId("wiki-page")
      .screenshot({ path: `${SHOTS}/03-repo-wiki.png` });

    await page.getByTestId("channel-engineering").click();
    await expect(page.getByTestId("message-composer")).toBeVisible();
    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/04-office-header-composer.png`,
      clip: { x: 256, y: 0, width: 1024, height: 720 },
    });
  });
});
