import { expect, type Page } from "@playwright/test";

// Crew fork delta vs upstream: retain this helper and its call sites when syncing Projects E2E specs.
export async function expandProjectPlumbing(page: Page): Promise<void> {
  const plumbing = page.getByTestId("project-plumbing");
  await expect(plumbing).toBeVisible();
  if ((await plumbing.getAttribute("open")) === null) {
    await plumbing.locator("summary").click();
  }
  await expect(plumbing).toHaveAttribute("open", "");
  await expect(plumbing.getByRole("tab").first()).toBeVisible();
}
