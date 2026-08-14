import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

const SHOTS = "test-results/crew-theme";
const THEME_STORAGE_KEY = "buzz-theme";
const SYNTAX_STORAGE_KEY = "buzz-syntax-theme";
const SPLIT_KEY = "buzz-theme-split-v1";
const FOLLOW_SYSTEM_KEY = "buzz-follow-system";

async function seedChrome(
  page: Page,
  chrome: "crew-dark" | "crew-light",
  syntax = "dark-plus",
) {
  await page.addInitScript(
    ({ chrome, syntax, keys }) => {
      window.localStorage.setItem(keys.theme, chrome);
      window.localStorage.setItem(keys.syntax, syntax);
      window.localStorage.setItem(keys.split, "true");
      window.localStorage.setItem(keys.follow, "false");
    },
    {
      chrome,
      syntax,
      keys: {
        theme: THEME_STORAGE_KEY,
        syntax: SYNTAX_STORAGE_KEY,
        split: SPLIT_KEY,
        follow: FOLLOW_SYSTEM_KEY,
      },
    },
  );
}

async function openGeneral(page: Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("app-sidebar")).toBeVisible();
}

test("Crew Dark is the default chrome: .dark, no sidebar gradient", async ({
  page,
}) => {
  await installMockBridge(page);
  await page.goto("/");
  await expect(page.locator("html")).toHaveClass(/dark/);
  await expect(page.locator("html")).toHaveAttribute(
    "data-crew-chrome",
    "crew-dark",
  );
  await expect(page.locator("html")).not.toHaveAttribute("data-buzz-sidebar");
  await expect(page.locator(".buzz-theme-gradient-underlay")).toHaveCSS(
    "background-image",
    "none",
  );
});

test("theme switch persists Crew Light and keeps syntax independent", async ({
  page,
}) => {
  await seedChrome(page, "crew-dark", "dark-plus");
  await installMockBridge(page);
  await page.goto("/");
  await openSettings(page, "appearance");

  await expect(page.getByTestId("appearance-chrome-row")).toBeVisible();
  await expect(page.getByTestId("appearance-syntax-row")).toBeVisible();
  await page.getByTestId("appearance-chrome-crew-light").click();

  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("buzz-theme")))
    .toBe("crew-light");
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("buzz-syntax-theme")))
    .toBe("dark-plus");
  await expect(page.locator("html")).toHaveClass(/light/);
  await expect(page.locator("html")).toHaveAttribute(
    "data-crew-chrome",
    "crew-light",
  );
});

test("syntax switch keeps chrome and updates the live preview", async ({
  page,
}) => {
  await seedChrome(page, "crew-dark", "dark-plus");
  await installMockBridge(page);
  await page.goto("/");
  await openSettings(page, "appearance");

  await page.getByTestId("syntax-theme-trigger").click();
  await page.getByTestId("syntax-theme-dracula").click();

  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("buzz-syntax-theme")))
    .toBe("dracula");
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("buzz-theme")))
    .toBe("crew-dark");
  await expect(page.locator("html")).toHaveAttribute(
    "data-crew-chrome",
    "crew-dark",
  );
  await expect(page.getByTestId("syntax-theme-preview")).toBeVisible();
});

test("migrates a stored Shiki chrome pref and shows a one-time toast", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem("buzz-theme", "catppuccin-macchiato");
  });
  await installMockBridge(page);
  await page.goto("/");

  await expect(page.locator("html")).toHaveAttribute(
    "data-crew-chrome",
    "crew-dark",
  );
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("buzz-theme")))
    .toBe("crew-dark");
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("buzz-syntax-theme")))
    .toBe("catppuccin-macchiato");
  await expect
    .poll(() =>
      page.evaluate(() => localStorage.getItem("buzz-theme-split-v1")),
    )
    .toBe("true");
  await expect(
    page.locator("[data-sonner-toast]").filter({
      hasText:
        "Your theme is now Crew Dark; code blocks kept Catppuccin Macchiato",
    }),
  ).toBeVisible();
});

test("community theme preference stays isolated from the chrome/syntax split", async ({
  page,
}) => {
  const pubkey = "deadbeef".repeat(8);
  await page.addInitScript(
    ({ owner }) => {
      window.localStorage.setItem("buzz-theme", "crew-dark");
      window.localStorage.setItem("buzz-syntax-theme", "dark-plus");
      window.localStorage.setItem("buzz-theme-split-v1", "true");
      const key = `buzz-community-theme.v1:${owner}:${encodeURIComponent("ws://localhost:3000")}`;
      window.localStorage.setItem(
        key,
        JSON.stringify({
          version: 1,
          theme: "crew-dark",
          syntax: "houston",
          accent: "#3b82f6",
          followSystem: false,
        }),
      );
    },
    { owner: pubkey },
  );
  await installMockBridge(page);
  await page.goto("/");
  await expect
    .poll(() =>
      page.evaluate((owner) => {
        const key = `buzz-community-theme.v1:${owner}:${encodeURIComponent("ws://localhost:3000")}`;
        return window.localStorage.getItem(key);
      }, pubkey),
    )
    .toContain("houston");
});

test("screenshot suite: Crew Dark sidebar, timeline, settings, syntax preview", async ({
  page,
}) => {
  await seedChrome(page, "crew-dark", "dracula");
  await installMockBridge(page);
  await openGeneral(page);
  await waitForAnimations(page);

  await page.getByTestId("app-sidebar").screenshot({
    path: `${SHOTS}/01-crew-dark-sidebar.png`,
  });
  await page.getByTestId("channel-drop-zone").screenshot({
    path: `${SHOTS}/02-crew-dark-timeline.png`,
  });

  const replyButton = page.locator('[data-testid^="reply-message-"]').first();
  if (await replyButton.isVisible()) {
    await replyButton.click({ force: true });
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("message-thread-panel").screenshot({
      path: `${SHOTS}/06-crew-dark-thread.png`,
    });
    const toolPane = page.getByTestId("tool-pane-tabs");
    if (await toolPane.isVisible()) {
      await page
        .locator(
          "[data-testid='thread-pr-hub'], [data-testid='tool-pane-tabs']",
        )
        .first()
        .screenshot({
          path: `${SHOTS}/07-crew-dark-tool-pane.png`,
        });
    }
  }

  await openSettings(page, "appearance");
  await expect(page.getByTestId("appearance-chrome-row")).toBeVisible();
  await expect(page.getByTestId("syntax-theme-preview")).toBeVisible();
  await waitForAnimations(page);
  await page.getByTestId("settings-theme").screenshot({
    path: `${SHOTS}/03-crew-dark-appearance.png`,
  });
});

test("screenshot suite: Crew Light sidebar and timeline", async ({ page }) => {
  await seedChrome(page, "crew-light", "dark-plus");
  await installMockBridge(page);
  await openGeneral(page);
  await waitForAnimations(page);

  await page.getByTestId("app-sidebar").screenshot({
    path: `${SHOTS}/04-crew-light-sidebar.png`,
  });
  await page.getByTestId("channel-drop-zone").screenshot({
    path: `${SHOTS}/05-crew-light-timeline.png`,
  });
});
