import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

test.describe("Org product removed (#233)", () => {
  test("sidebar has no Org entry and /org redirects home", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/");
    await expect(page.getByTestId("app-sidebar")).toBeVisible();
    await expect(page.getByTestId("open-org-view")).toHaveCount(0);

    await page.goto("/org");
    await expect(page).toHaveURL(/#\/$/);
    await expect(page.getByTestId("org-view")).toHaveCount(0);
    await expect(page.getByTestId("app-sidebar")).toBeVisible();
  });
});
