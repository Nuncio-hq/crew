import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { FEATURE_OVERRIDES_STORAGE_KEY } from "../helpers/features";

const GENERAL_CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const MOCK_IDENTITY_PUBKEY = "deadbeef".repeat(8);
const COWORK_DTAG = "cowork-docs";
const COWORK_REPO = `30617:${MOCK_IDENTITY_PUBKEY}:${COWORK_DTAG}`;
const THREAD_ID = "c".repeat(64);
const SHOTS = "test-results/cowork-projects";

test.use({ video: "on", viewport: { width: 1280, height: 720 } });

async function seedCoworkProject(page: Page) {
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
          id: "mock-cowork-repo",
          kind: 30617,
          pubkey,
          created_at: Math.floor(Date.now() / 1000),
          content: "Q3 proposals",
          tags: [
            ["d", "cowork-docs"],
            ["name", "Q3 proposals"],
            ["buzz-channel", channelId],
            ["buzz-location", "local", "/tmp/cowork-docs"],
            ["crew-workspace-mode", "folder"],
          ],
        },
      ];
      win.__BUZZ_E2E_PROJECT_GIT_PROBE__ = {
        isGit: false,
        defaultBranch: null,
        currentBranch: null,
        dirty: false,
        uncommittedCount: 0,
        localBranches: [],
        remoteBranches: [],
      };
    },
    {
      channelId: GENERAL_CHANNEL_ID,
      featureKey: FEATURE_OVERRIDES_STORAGE_KEY,
      pubkey: MOCK_IDENTITY_PUBKEY,
    },
  );
}

test("Cowork channel has no workspace selector and shows the cowork chip", async ({
  page,
}) => {
  await seedCoworkProject(page);
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: TEST_IDENTITIES.alice.pubkey,
        name: "Hermes",
        status: "running",
        channelNames: ["general"],
      },
    ],
  });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toContainText("general");
  await expect(page.getByTestId("composer-workspace-selector")).toHaveCount(0);

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
  await page.evaluate(
    ({ repo, threadId }) => {
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        id: threadId,
        content:
          `[ctx]: <buzz://project-workspace?repo=${encodeURIComponent(repo)}&path=%2Ftmp%2Fcowork-docs&mode=folder> "Agents work in this folder."\n\n` +
          `@Hermes draft the proposal.`,
      });
    },
    { repo: COWORK_REPO, threadId: THREAD_ID },
  );
  const chip = page.getByTestId("project-thread-cowork-chip");
  await expect(chip).toBeVisible();
  await expect(chip).toHaveText(/cowork/);
  await expect(
    page.getByTestId("project-thread-badge-chips"),
  ).not.toContainText(/#\d+/);
  await waitForAnimations(page);
  await page
    .getByTestId("project-thread-badge-chips")
    .screenshot({ path: `${SHOTS}/01-cowork-chip.png` });
});

test("Versions timeline lists turns and restore adds a checkpoint", async ({
  page,
}) => {
  await seedCoworkProject(page);
  await installMockBridge(page);
  await page.goto(`/#/projects/${COWORK_REPO}`, {
    waitUntil: "domcontentloaded",
  });
  const timeline = page.getByTestId("cowork-versions-timeline");
  await expect(timeline).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("cowork-version-entry")).toHaveCount(2);
  await expect(timeline).toContainText(
    "Turn 1 — Hermes · thread 'Q3 proposal'",
  );
  await expect(timeline).toContainText("External changes");
  await expect(page.getByTestId("cowork-excluded-notice")).toContainText(
    /not versioned/i,
  );
  await waitForAnimations(page);
  await timeline.screenshot({ path: `${SHOTS}/02-versions-timeline.png` });

  await page.getByTestId("cowork-restore-file").first().click();
  await expect(page.getByTestId("cowork-version-entry")).toHaveCount(3);
  await expect(timeline).toContainText("Restored version");
  await waitForAnimations(page);
  await timeline.screenshot({ path: `${SHOTS}/03-file-restore.png` });
});
