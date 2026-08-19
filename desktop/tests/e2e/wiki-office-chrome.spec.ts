import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const SHOTS = "test-results/wiki-office-chrome";

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

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

test.describe("Wiki office chrome (#221)", () => {
  test("wiki home IA and repo wiki ask are distinct surfaces", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/");

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
      .screenshot({ path: `${SHOTS}/01-wiki-home.png` });

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
      .screenshot({ path: `${SHOTS}/02-repo-wiki.png` });

    await page.getByTestId("channel-engineering").click();
    await expect(page.getByTestId("message-composer")).toBeVisible();
    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/03-office-header-composer.png`,
      clip: { x: 256, y: 0, width: 1024, height: 720 },
    });
  });
});
