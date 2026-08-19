import { expect, test } from "@playwright/test";
import path from "node:path";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const OUT = path.join("test-results", "org-removed-233");

test.use({ video: "on", viewport: { width: 1280, height: 720 } });

test.describe("Org product removed (#233)", () => {
  test("sidebar has no Org entry and /org redirects home", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/");
    await expect(page.getByTestId("app-sidebar")).toBeVisible();
    await expect(page.getByTestId("open-org-view")).toHaveCount(0);
    await expect(page.getByTestId("open-wiki-view")).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("app-sidebar").screenshot({
      path: path.join(OUT, "01-sidebar-no-org.png"),
    });

    // Hash history — path routes must use /#/…
    await page.goto("/#/org");
    await expect(page).toHaveURL(/#\/$/);
    await expect(page.getByTestId("org-view")).toHaveCount(0);
    await expect(page.getByTestId("app-sidebar")).toBeVisible();
    await waitForAnimations(page);
    await page.screenshot({
      path: path.join(OUT, "02-org-redirect-inbox.png"),
      clip: { x: 0, y: 0, width: 1280, height: 720 },
    });
  });
});
