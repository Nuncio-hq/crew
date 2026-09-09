import { expect, test } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { installMockBridge } from "../helpers/bridge";
import { FEATURE_OVERRIDES_STORAGE_KEY } from "../helpers/features";
import { waitForAnimations } from "../helpers/animations";

const shots = process.env.CREW_SHELL_SCREENSHOTS;
test("stable shell preserves workspace browsing, Company Wiki, footer settings and channel history", async ({
  page,
}) => {
  await page.addInitScript((key) => {
    localStorage.setItem(
      key,
      JSON.stringify({ projects: false, workflows: false, pulse: false }),
    );
  }, FEATURE_OVERRIDES_STORAGE_KEY);
  await installMockBridge(page, undefined, { seedPreviewFeatures: false });
  await page.goto("/");
  const sidebar = page.getByTestId("app-sidebar");
  await expect(sidebar).toBeVisible();
  await expect(page.getByTestId("open-workflows-view")).toBeVisible();
  await expect(page.getByTestId("sidebar-projects-section")).toBeVisible();
  await expect(sidebar.getByTestId("open-wiki-view")).toHaveCount(0);
  await expect(sidebar.getByTestId("stream-list")).toHaveCount(0);
  await page.getByTestId("workspace-menu-trigger").click();
  await expect(page.getByTestId("workspace-browse-channels")).toBeVisible();
  await expect(page.getByTestId("workspace-company-wiki")).toBeVisible();
  await expect(page.getByTestId("open-pulse-view")).toHaveCount(0);
  if (shots) {
    await mkdir(shots, { recursive: true });
    await waitForAnimations(page);
    await page.screenshot({
      path: `${shots}/workspace-menu.png`,
      clip: { x: 0, y: 0, width: 370, height: 650 },
    });
  }
  await page.getByTestId("workspace-browse-channels").click();
  await expect(page.getByTestId("channel-browser-dialog")).toBeVisible();
  await page
    .getByTestId("browse-channel-general")
    .getByRole("button")
    .first()
    .click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.getByTestId("workspace-menu-trigger").click();
  await page.getByTestId("workspace-company-wiki").click();
  await expect(page).toHaveURL(/#\/wiki$/);
  await page.getByTestId("global-back").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await page.getByTestId("sidebar-settings").click();
  await expect(page).toHaveURL(/#\/settings/);
});

test("Project chevron only expands while Wiki preserves the exact repository coordinate", async ({
  page,
}) => {
  const owner = "deadbeef".repeat(8);
  const repositoryAddress = `30617:${owner}:buzz`;
  await installMockBridge(page);
  await page.goto("/");
  await page.getByTestId("sidebar-projects-settings").click();
  await page.getByRole("menuitem", { name: "Show", exact: true }).hover();
  await page.getByRole("menuitemradio", { name: "Owned by me" }).click();
  const row = page.getByTestId("sidebar-project-buzz");
  await expect(row).toBeVisible();
  const before = page.url();
  await page.getByTestId("sidebar-project-expand-buzz").click();
  await expect(page).toHaveURL(before);
  await expect(
    page.getByTestId("sidebar-project-channel-buzz-buzz"),
  ).toBeVisible();
  const wiki = page.getByTestId(`sidebar-project-wiki-30621:${owner}:buzz`);
  await expect(wiki).toBeVisible();
  await wiki.click();
  await expect
    .poll(() =>
      new URLSearchParams(page.url().split("?")[1]).get("repositoryAddress"),
    )
    .toBe(repositoryAddress);
  await expect
    .poll(() => new URLSearchParams(page.url().split("?")[1]).get("tab"))
    .toBe("wiki");
  await row.click();
  await expect
    .poll(() => new URLSearchParams(page.url().split("?")[1]).get("tab"))
    .toBe(null);
});
