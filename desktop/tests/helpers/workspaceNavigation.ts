import { expect, type Page } from "@playwright/test";

/** Navigate through the production workspace menu without depending on sidebar pins. */
export async function openWorkspaceChannel(page: Page, name: string) {
  await page.getByTestId("workspace-menu-trigger").click();
  await page.getByTestId("workspace-browse-channels").click();
  await expect(page.getByTestId("channel-browser-dialog")).toBeVisible();
  await page
    .getByTestId(`browse-channel-${name}`)
    .getByRole("button")
    .first()
    .click();
  await expect(page.getByTestId("chat-title")).toContainText(name);
}
