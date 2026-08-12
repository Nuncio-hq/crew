import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

const SHOTS = "test-results/worktree-storage";

test.describe("worktree storage reclaim (#174)", () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test("renders candidates with dual clocks, refusals, and bulk outcomes", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await openSettings(page, "storage");

    const card = page.getByTestId("settings-storage");
    await expect(card).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId("storage-suggestion")).toContainText(
      "reclaimable",
    );
    await expect(page.getByTestId("storage-absence-banner")).toContainText(
      "You were away 6 days",
    );

    const lean = page.getByTestId("storage-row-fix-reconnect-freeze");
    await expect(lean).toBeVisible();
    await expect(lean.getByTestId("storage-row-reason")).toContainText(
      "PR #167 merged",
    );
    await expect(lean).toContainText("observed hr");

    const hibernate = page.getByTestId("storage-row-scroll-history-fix");
    await expect(hibernate).toHaveAttribute("data-tier", "hibernate");
    await expect(hibernate.getByTestId("storage-row-reason")).toContainText(
      "Hibernate",
    );

    const refused = page.getByTestId("storage-row-issue-116-roles");
    await expect(refused).toHaveAttribute("data-candidate", "false");
    await expect(refused.getByTestId("storage-row-refusal")).toContainText(
      "dirty",
    );

    await waitForAnimations(page);
    await card.screenshot({ path: `${SHOTS}/01-storage-view.png` });

    await page.getByTestId("storage-run-cleanup").click();
    await expect(lean.getByTestId("storage-row-outcome")).toContainText(
      /freed|✓/i,
      { timeout: 10_000 },
    );
    await expect(hibernate.getByTestId("storage-row-outcome")).toContainText(
      /freed|✓/i,
    );

    await expect
      .poll(async () =>
        page.evaluate(() =>
          (window.__BUZZ_E2E_COMMAND_LOG__ ?? [])
            .map((entry) => entry.command)
            .filter((command) =>
              [
                "revalidate_worktree_storage_action",
                "clear_project_worktree_cache",
                "evict_project_worktree",
              ].includes(command),
            ),
        ),
      )
      .toEqual(
        expect.arrayContaining([
          "revalidate_worktree_storage_action",
          "clear_project_worktree_cache",
          "evict_project_worktree",
        ]),
      );

    await waitForAnimations(page);
    await card.screenshot({ path: `${SHOTS}/02-after-bulk-run.png` });
  });

  test("bulk run surfaces mid-run lease refusal as a skip outcome", async ({
    page,
  }) => {
    const now = Math.floor(Date.now() / 1000);
    await installMockBridge(page, {
      worktreeStorageSnapshot: {
        recentAbsenceSecs: 0,
        rows: [
          {
            repositoryPath: "/tmp/crew-fixture",
            worktreePath: "/tmp/.buzz-worktrees/lease-race",
            worktreeName: "lease-race",
            branch: "buzz/dddddddddddd",
            rootEventId: "d".repeat(64),
            routingChannelId: "11111111-1111-1111-1111-111111111111",
            lifecycleIdentity: "verified",
            prNumber: 200,
            prState: "MERGED",
            prTitle: "Lease race",
            lastUsedAt: now - 86_400,
            observedIdleSecs: 50 * 3600,
            wallIdleSecs: 86_400,
            dirty: false,
            busy: true,
            branchPushed: true,
            diskBytes: 1_000_000_000,
            cacheBytes: 900_000_000,
            checkoutBytes: 100_000_000,
            cacheCategoryIds: ["cargo-target"],
            candidate: true,
            tier: "lean",
            reason: "PR #200 merged — Lean: sweep cache",
            readOnly: false,
            refusalReason: "An agent is using this worktree.",
            canClearCache: false,
            canEvict: false,
          },
        ],
        candidateCount: 1,
        reclaimableBytes: 900_000_000,
        totalDiskBytes: 1_000_000_000,
        totalCacheBytes: 900_000_000,
      },
    });
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await openSettings(page, "storage");
    await expect(page.getByTestId("settings-storage")).toBeVisible();
    await page.getByTestId("storage-run-cleanup").click();
    await expect(
      page
        .getByTestId("storage-row-lease-race")
        .getByTestId("storage-row-outcome"),
    ).toContainText(/skipped/i, { timeout: 10_000 });
  });
});
