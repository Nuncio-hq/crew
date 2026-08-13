import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { FEATURE_OVERRIDES_STORAGE_KEY } from "../helpers/features";

const GENERAL_CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const MOCK_IDENTITY_PUBKEY = "deadbeef".repeat(8);
const SHOTS = "test-results/workspace-binding-selector";

test.use({ video: "on", viewport: { width: 1280, height: 720 } });

async function seedGitProjectChannel(page: Page) {
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
        }>;
        __BUZZ_E2E_PROJECT_GIT_PROBE__?: {
          isGit: boolean;
          defaultBranch: string | null;
          currentBranch: string | null;
          dirty: boolean;
          uncommittedCount: number;
          localBranches: string[];
          remoteBranches: string[];
        };
      };
      win.__BUZZ_E2E_EXTRA_PROJECT_EVENTS__ = [
        {
          id: "mock-workspace-binding-repo",
          kind: 30617,
          pubkey,
          created_at: Math.floor(Date.now() / 1000),
          content: "Crew workspace",
          tags: [
            ["d", "workspace-binding"],
            ["name", "Crew"],
            ["buzz-channel", channelId],
            ["buzz-location", "local", "/tmp/crew"],
            ["clone", "https://github.com/Nuncio-hq/crew.git"],
          ],
        },
      ];
      win.__BUZZ_E2E_PROJECT_GIT_PROBE__ = {
        isGit: true,
        defaultBranch: "main",
        currentBranch: "main",
        dirty: false,
        uncommittedCount: 0,
        localBranches: ["main", "release"],
        remoteBranches: ["main"],
      };
    },
    {
      channelId: GENERAL_CHANNEL_ID,
      featureKey: FEATURE_OVERRIDES_STORAGE_KEY,
      pubkey: MOCK_IDENTITY_PUBKEY,
    },
  );
}

test("selector is only in git Project channels and default send matches today", async ({
  page,
}) => {
  await seedGitProjectChannel(page);
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: TEST_IDENTITIES.alice.pubkey,
        name: "Claude Opus",
        status: "running",
        channelNames: ["general"],
      },
    ],
  });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toContainText("general");
  const selector = page.getByTestId("composer-workspace-selector");
  await expect(selector).toBeVisible();
  await expect(selector).toHaveText(/New worktree/);
  await waitForAnimations(page);
  await selector.screenshot({ path: `${SHOTS}/01-default-new-worktree.png` });

  await selector.click();
  await expect(page.getByTestId("composer-workspace-new")).toBeVisible();
  await expect(page.getByTestId("composer-workspace-main")).toBeVisible();
  await expect(page.getByTestId("composer-workspace-base-main")).toBeVisible();
  await expect(
    page.getByTestId("composer-workspace-branch-release"),
  ).toBeVisible();
  await waitForAnimations(page);
  await page.screenshot({
    path: `${SHOTS}/02-selector-menu.png`,
    clip: { x: 700, y: 420, width: 520, height: 300 },
  });

  await page.getByTestId("composer-workspace-main").click();
  await expect(selector).toHaveText(/Main checkout/);
  await selector.click();
  await page.getByTestId("composer-workspace-branch-release").click();
  await expect(selector).toHaveText(/release/);
  await selector.click();
  await page.getByTestId("composer-workspace-new").click();
  await selector.click();
  await page.getByTestId("composer-workspace-base-release").click();
  await expect(selector).toHaveText(/release/);

  await page.getByTestId("channel-engineering").click();
  await expect(page.getByTestId("chat-title")).toContainText("engineering");
  await expect(page.getByTestId("composer-workspace-selector")).toHaveCount(0);

  await page.getByTestId("channel-general").click();
  await expect(selector).toBeVisible();
  await selector.click();
  await page.getByTestId("composer-workspace-new").click();

  const input = page.getByTestId("message-input");
  await input.fill("@Cl");
  await expect(
    page
      .getByTestId("message-composer")
      .getByTestId("mention-autocomplete")
      .locator("button", { hasText: "Claude Opus" }),
  ).toBeVisible();
  await input.press("Enter");
  await page.keyboard.type(" inspect the default workspace");
  await page.getByTestId("send-message").click();
  await expect
    .poll(async () =>
      page.evaluate(() => window.__BUZZ_E2E_SIGNED_EVENTS__?.length ?? 0),
    )
    .toBeGreaterThan(0);
  const defaultUrl = await page.evaluate(() => {
    const events = window.__BUZZ_E2E_SIGNED_EVENTS__ ?? [];
    const match = events
      .map((event) => event.content)
      .find((content) => content.includes("buzz://project-workspace?"));
    return match ?? "";
  });
  expect(defaultUrl).toContain("buzz://project-workspace?");
  expect(defaultUrl).not.toMatch(/[?&]ws=/);
  expect(defaultUrl).not.toMatch(/[?&]base=/);

  await selector.click();
  await page.getByTestId("composer-workspace-main").click();
  await input.fill("@Cl");
  await expect(
    page
      .getByTestId("message-composer")
      .getByTestId("mention-autocomplete")
      .locator("button", { hasText: "Claude Opus" }),
  ).toBeVisible();
  await input.press("Enter");
  await page.keyboard.type(" use the live checkout");
  await page.getByTestId("send-message").click();
  const mainUrl = await page.evaluate(() => {
    const events = window.__BUZZ_E2E_SIGNED_EVENTS__ ?? [];
    return (
      events
        .map((event) => event.content)
        .reverse()
        .find((content) => content.includes("ws=main")) ?? ""
    );
  });
  expect(mainUrl).toContain("&ws=main");
});

test("thread chips render each workspace binding", async ({ page }) => {
  await seedGitProjectChannel(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("channel-general").click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
            channelName: "general",
          }) ?? false,
      ),
    )
    .toBe(true);
  await page.evaluate(() => {
    window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "general",
      content:
        `[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew&ws=main>\n\n` +
        `@Claude Opus inspect the live checkout.`,
    });
    window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
      channelName: "general",
      content:
        `[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew&ws=branch:release>\n\n` +
        `@Claude Opus continue the release branch.`,
    });
  });
  await expect(page.getByTestId("project-thread-badge-chips")).toHaveCount(2);
  await expect(
    page.getByTestId("project-thread-badge-branch-text").first(),
  ).toHaveText(/main/);
  await waitForAnimations(page);
  await page
    .getByTestId("project-thread-badge-chips")
    .first()
    .screenshot({ path: `${SHOTS}/03-main-binding-chip.png` });
});
