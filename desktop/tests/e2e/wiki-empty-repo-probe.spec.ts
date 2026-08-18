import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const SHOTS = "test-results/wiki-empty-repo-probe";
const OWNER = "deadbeef".repeat(8);
const CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const EMPTY_COPY =
  "Empty repo / no default branch. Push to main, then Generate.";

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

async function seedNuncioCrewUnbound(page: Page) {
  await page.addInitScript(
    ({ channelId, owner }) => {
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
      const now = Math.floor(Date.now() / 1000);
      const sig = "mocksig".repeat(20).slice(0, 128);
      win.__BUZZ_E2E_EXTRA_PROJECT_EVENTS__ = [
        {
          id: "222a0000".padEnd(64, "0"),
          kind: 30617,
          pubkey: owner,
          created_at: now,
          content: "NuncioCrew thin fork of Buzz.",
          tags: [
            ["d", "nunciocrew"],
            ["name", "NuncioCrew"],
            ["description", "NuncioCrew thin fork of Buzz."],
            ["clone", "https://github.com/Nuncio-hq/crew.git"],
            ["default-branch", "main"],
            ["buzz-channel", channelId],
          ],
          sig,
        },
        {
          id: "222a0001".padEnd(64, "0"),
          kind: 30618,
          pubkey: owner,
          created_at: now,
          content: "",
          tags: [
            ["d", "nunciocrew"],
            ["HEAD", "ref: refs/heads/main"],
            ["refs/heads/main", "39c94cf005a233df12b3116c1305bb286015bc6f"],
          ],
          sig,
        },
      ];
    },
    { channelId: CHANNEL_ID, owner: OWNER },
  );
}

test.describe("Wiki empty-repo probe (#222)", () => {
  test("unbound NuncioCrew card with empty-repo job does not blame GitHub", async ({
    page,
  }) => {
    await seedNuncioCrewUnbound(page);
    await installMockBridge(page);
    await page.goto("/");
    await page.getByTestId("open-wiki-view").click();
    await expect(page.getByTestId("wiki-library")).toBeVisible();
    await expect(page.getByTestId("wiki-repo-card-NuncioCrew")).toBeVisible();

    await page.evaluate((owner) => {
      window.__BUZZ_E2E_SET_WIKI_JOB__?.({
        repoKey: `${owner}:nunciocrew`.toLowerCase(),
        status: "idle",
        done: 0,
        total: 0,
        error: "empty-repo",
        costNote: null,
      });
    }, OWNER);

    const card = page.getByTestId("wiki-repo-card-NuncioCrew");
    await expect(card.getByTestId("wiki-empty-repo")).toHaveCount(0);
    await expect(card).not.toContainText(EMPTY_COPY);
    await expect(card.getByTestId("wiki-missing-local")).toHaveText(
      "No local checkout found.",
    );
    await expect(card.getByTestId("wiki-generate-NuncioCrew")).toBeVisible();

    await waitForAnimations(page);
    await card.screenshot({
      path: `${SHOTS}/01-nunciocrew-unbound-not-empty-github.png`,
    });
  });
});
