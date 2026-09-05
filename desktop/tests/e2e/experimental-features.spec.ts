import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { FEATURE_OVERRIDES_STORAGE_KEY } from "../helpers/features";
import { openSettings } from "../helpers/settings";

test("thread isolation stays mandatory without a session-scope experiment", async ({
  page,
}) => {
  await page.addInitScript((key) => {
    window.localStorage.setItem(
      key,
      JSON.stringify({ threadScopedAcpSessions: false }),
    );
  }, FEATURE_OVERRIDES_STORAGE_KEY);
  await installMockBridge(page, undefined, { seedPreviewFeatures: false });
  await page.goto("/");
  await openSettings(page, "experimental");

  await expect(
    page.getByTestId("feature-toggle-threadScopedAcpSessions"),
  ).toHaveCount(0);
  await expect(page.getByTestId("feature-toggle-projects")).not.toBeChecked();
  await expect(page.getByTestId("feature-toggle-workflows")).not.toBeChecked();
  await page.reload();
  await expect(page.getByTestId("settings-view")).toBeVisible();
  await expect(
    page.getByTestId("feature-toggle-threadScopedAcpSessions"),
  ).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        window.__BUZZ_E2E_COMMAND_LOG__?.filter(
          ({ command }) => command === "set_thread_scoped_acp_sessions",
        ) ?? [],
    ),
  ).toEqual([]);
});
