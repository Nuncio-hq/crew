import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";
import { FEATURE_OVERRIDES_STORAGE_KEY } from "../helpers/features";

const SHOTS = "test-results/channels-only-sidebar";
const ENGINEERING_ID = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const OWNER = "deadbeef".repeat(8);
const SIDEBAR_CLIP = { x: 0, y: 0, width: 280, height: 720 };

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

/**
 * Exclusive repo binding on #engineering. Pre-#223 this pulled the channel
 * into a Projects folder and hid it from Channels. NuncioCrew is entered as
 * that office channel, not a project folder.
 */
async function seedExclusiveProjectOnEngineering(page: Page) {
  await page.addInitScript(
    ({ channelId, featureKey, pubkey }) => {
      window.localStorage.setItem(
        featureKey,
        JSON.stringify({ projects: true }),
      );
      const win = window as typeof window & {
        __BUZZ_E2E_EXTRA_PROJECT_EVENTS__?: Array<{
          id: string;
          kind: number;
          pubkey: string;
          created_at: number;
          content: string;
          tags: string[][];
          sig: string;
        }>;
      };
      win.__BUZZ_E2E_EXTRA_PROJECT_EVENTS__ = [
        {
          id: "aa".repeat(32),
          kind: 30617,
          pubkey,
          created_at: Math.floor(Date.now() / 1000),
          content: "NuncioCrew",
          tags: [
            ["d", "nunciocrew"],
            ["name", "NuncioCrew project"],
            ["buzz-channel", channelId],
            ["clone", "https://github.com/Nuncio-hq/crew.git"],
          ],
          sig: "mocksig".repeat(20).slice(0, 128),
        },
      ];
    },
    {
      channelId: ENGINEERING_ID,
      featureKey: FEATURE_OVERRIDES_STORAGE_KEY,
      pubkey: OWNER,
    },
  );
}

test.describe("CompanyOS workspace rail (#223)", () => {
  test("rail is Inbox + Agents + Workflows + Projects + DMs", async ({
    page,
  }) => {
    await seedExclusiveProjectOnEngineering(page);
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    const sidebar = page.getByTestId("app-sidebar");
    await expect(sidebar).toBeVisible();
    const primaryMenu = page.getByTestId("sidebar-primary-menu");
    await expect(primaryMenu).toBeVisible();
    await expect(
      primaryMenu.getByRole("button", { name: "Inbox" }),
    ).toBeVisible();
    await expect(
      primaryMenu.getByRole("button", { name: "Agents" }),
    ).toBeVisible();
    await expect(
      primaryMenu.getByRole("button", { name: "Workflows" }),
    ).toBeVisible();

    await expect(page.getByTestId("sidebar-projects-section")).toBeVisible();
    await expect(page.getByTestId("sidebar-projects-section-label")).toHaveText(
      "Projects",
    );
    await expect(page.getByTestId("work-tree-folder-engineering")).toHaveCount(
      0,
    );
    await expect(page.getByTestId("open-workbench-view")).toHaveCount(0);
    await expect(
      primaryMenu.getByRole("button", { name: "Workbench" }),
    ).toHaveCount(0);
    // Ordinary channels are intentionally not a flat sidebar section. They
    // remain reachable from the workspace menu, including project-linked
    // channels and channels with no project metadata.
    await expect(page.getByTestId("stream-list")).toHaveCount(0);
    await expect(page.getByTestId("dm-list")).toBeVisible();

    await page.getByTestId("workspace-menu-trigger").click();
    await expect(page.getByTestId("workspace-browse-channels")).toBeVisible();
    await page.getByTestId("workspace-browse-channels").click();
    await expect(page.getByTestId("channel-browser-dialog")).toBeVisible();
    await page
      .getByTestId("browse-channel-engineering")
      .getByRole("button")
      .first()
      .click();
    await expect(page.getByTestId("chat-title")).toContainText("engineering");
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));

    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/01-channels-only-sidebar.png`,
      clip: SIDEBAR_CLIP,
    });
  });
});
