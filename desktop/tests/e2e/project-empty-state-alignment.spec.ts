import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const ALIGNMENT_TOLERANCE_PX = 2;

async function enableProjectsFeature(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "buzz-feature-overrides-v1",
      JSON.stringify({ projects: true }),
    );
  });
}

test("first-time project empty state opens project creation", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    window.__BUZZ_E2E_EMPTY_PROJECTS__ = true;
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  await expect(
    page.getByRole("main").getByText("No projects yet"),
  ).toBeVisible();
  await page.evaluate(() => {
    const bridge = window.__TAURI_INTERNALS__;
    if (!bridge) throw new Error("Native mock bridge unavailable");
    const invoke = bridge.invoke;
    bridge.invoke = async (command, args, options) => {
      if (command === "plugin:dialog|open") return "/tmp/first-project";
      return invoke(command, args, options);
    };
  });
  await page.getByRole("button", { name: "Create project" }).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText("/tmp/first-project");
  await expect(page.getByLabel("Repository name", { exact: true })).toHaveValue(
    "first-project",
  );
  await expect(
    dialog.getByRole("button", { name: "Add Repository", exact: true }),
  ).toBeEnabled();
});

test("project home context aligns with the channel header", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("channel-buzz").click();
  await expect(page.getByTestId("chat-title")).toHaveText("buzz");
  await expect(page.getByTestId("project-home-context-tasks")).toBeVisible();
  await waitForAnimations(page);

  const [headerTitleBox, tasksBox] = await Promise.all([
    page.getByTestId("chat-title").boundingBox(),
    page.getByTestId("project-home-context-tasks").boundingBox(),
  ]);
  expect(headerTitleBox).not.toBeNull();
  expect(tasksBox).not.toBeNull();
  const headerTitleCenter =
    (headerTitleBox?.y ?? 0) + (headerTitleBox?.height ?? 0) / 2;
  const tasksCenter = (tasksBox?.y ?? 0) + (tasksBox?.height ?? 0) / 2;
  expect(Math.abs(headerTitleCenter - tasksCenter)).toBeLessThanOrEqual(
    ALIGNMENT_TOLERANCE_PX,
  );
});
