import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const SHOTS = "test-results/wiki-corrective-navigation";
const OWNER = "deadbeef".repeat(8);
const SOURCE_PATH = "desktop/src/features/projects/ui/ProjectDetailScreen.tsx";
const LONG_WIKI_BODY = Array.from(
  { length: 18 },
  (_, index) =>
    `Scroll restoration proof section ${index + 1}: this seeded article keeps the reading surface long enough to exercise real overflow at both target viewports.`,
).join("\n\n");

test.describe.configure({ timeout: 90_000 });
test.use({ video: "on" });

async function openSeededRepoWiki(page: Page, contentSuffix = LONG_WIKI_BODY) {
  await installMockBridge(page);
  await page.goto("/");
  await expect(page.getByTestId("workspace-menu-trigger")).toBeVisible();
  await page.getByTestId("workspace-menu-trigger").click();
  await page.getByTestId("workspace-company-wiki").click();
  await expect(page).toHaveURL(/#\/wiki$/);
  await expect(page.getByTestId("wiki-library")).toBeVisible();

  await page.evaluate(
    ({ owner, contentSuffix }) => {
      window.__BUZZ_E2E_SEED_WIKI__?.({
        owner,
        repoD: "buzz",
        contentSuffix,
      });
      window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
        queryKey: ["crew-wiki-events"],
      });
    },
    { owner: OWNER, contentSuffix },
  );
  await expect(page.getByTestId("wiki-repo-card-buzz")).toContainText(
    "minutes",
  );
  await page
    .getByTestId("wiki-repo-card-buzz")
    .locator("button")
    .first()
    .click();
  await expect(page.getByTestId("wiki-page")).toBeVisible();
  await expect(page.getByTestId("wiki-page-scroll")).toBeVisible();
}

async function openSourceWithKeyboard(page: Page) {
  const sourceFiles = page.getByTestId("wiki-source-files");
  await sourceFiles.scrollIntoViewIfNeeded();
  const summary = sourceFiles.locator("summary");
  await summary.focus();
  await summary.press("Enter");
  const sourceButton = sourceFiles.getByRole("button", { name: SOURCE_PATH });
  await expect(sourceButton).toBeEnabled();
  await sourceButton.focus();
  await sourceButton.press("Enter");
  await expect(page.getByTestId("wiki-source-preview")).toContainText(
    "export function ProjectDetailScreen()",
  );
  await expect(page.getByTestId("wiki-source-pane")).toBeVisible();
  return sourceButton;
}

test.describe("Wiki corrective navigation (#364)", () => {
  test.describe("1130x1089", () => {
    test.use({ viewport: { width: 1130, height: 1089 } });

    test("reads, searches, opens exact source, and restores scroll after back", async ({
      page,
    }) => {
      await openSeededRepoWiki(page);

      await expect(page.getByTestId("wiki-toc")).toBeVisible();
      await expect(page.getByTestId("wiki-markdown")).toContainText(
        "verified snapshot",
      );

      const search = page.getByTestId("wiki-page-search");
      await search.focus();
      await search.press("ControlOrMeta+A");
      await search.pressSequentially("body search proof");
      await expect(page.getByTestId("wiki-search-results")).toBeVisible();
      const result = page.getByTestId("wiki-search-result-overview");
      await result.focus();
      await result.press("Enter");
      await expect(search).toHaveValue("");
      await expect(page.getByTestId("wiki-markdown")).toContainText(
        "body search proof",
      );

      const sourceButton = await openSourceWithKeyboard(page);
      await expect(page.getByTestId("wiki-toc")).toHaveCount(0);
      await page.keyboard.press("Escape");
      await expect(page.getByTestId("wiki-source-pane")).toHaveCount(0);
      await expect(sourceButton).toBeFocused();
      const command = await page.evaluate(() =>
        window.__BUZZ_E2E_COMMAND_PAYLOADS__?.find(
          (entry) => entry.command === "wiki_open_verified_source",
        ),
      );
      expect(command).toMatchObject({
        command: "wiki_open_verified_source",
        payload: {
          capabilityId: "e2e-wiki-source-buzz",
          referenceIndex: 0,
        },
      });

      const scroll = page.getByTestId("wiki-page-scroll");
      const targetScroll = await scroll.evaluate((element) =>
        Math.min(120, Math.max(0, element.scrollHeight - element.clientHeight)),
      );
      expect(targetScroll).toBeGreaterThan(0);
      await scroll.evaluate((element, value) => {
        element.scrollTop = value;
        element.dispatchEvent(new Event("scroll", { bubbles: true }));
      }, targetScroll);
      await expect
        .poll(() => scroll.evaluate((element) => element.scrollTop))
        .toBe(targetScroll);

      await page.getByRole("button", { name: "Back to wiki library" }).click();
      await expect(page.getByTestId("wiki-library")).toBeVisible();
      await page
        .getByTestId("wiki-repo-card-buzz")
        .locator("button")
        .first()
        .click();
      await expect(page.getByTestId("wiki-page")).toBeVisible();
      await expect
        .poll(() =>
          page
            .getByTestId("wiki-page-scroll")
            .evaluate((element) => element.scrollTop),
        )
        .toBe(targetScroll);

      await waitForAnimations(page);
      await page
        .getByTestId("wiki-page")
        .screenshot({ path: `${SHOTS}/01-1130x1089-read-search-source.png` });
    });

    test("retries restoration after delayed content growth and respects user scroll", async ({
      page,
    }) => {
      await openSeededRepoWiki(page, LONG_WIKI_BODY);
      const scroll = page.getByTestId("wiki-page-scroll");
      const targetScroll = await scroll.evaluate((element) =>
        Math.min(240, Math.max(0, element.scrollHeight - element.clientHeight)),
      );
      expect(targetScroll).toBeGreaterThan(0);
      await scroll.evaluate((element, value) => {
        element.scrollTop = value;
        element.dispatchEvent(new Event("scroll", { bubbles: true }));
      }, targetScroll);

      await page.getByRole("button", { name: "Back to wiki library" }).click();
      await expect(page.getByTestId("wiki-library")).toBeVisible();
      await page.evaluate((owner) => {
        window.__BUZZ_E2E_SEED_WIKI__?.({ owner, repoD: "buzz" });
        window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
          queryKey: ["crew-wiki-events"],
        });
      }, OWNER);
      await page
        .getByTestId("wiki-repo-card-buzz")
        .locator("button")
        .first()
        .click();
      await expect(page.getByTestId("wiki-page")).toBeVisible();
      await expect(page.getByTestId("wiki-markdown")).not.toContainText(
        "Scroll restoration proof section 18",
      );
      await expect
        .poll(() => scroll.evaluate((element) => element.scrollTop))
        .toBe(0);

      await page.waitForTimeout(450);
      await page.evaluate(
        ({ owner, contentSuffix }) => {
          window.__BUZZ_E2E_SEED_WIKI__?.({
            owner,
            repoD: "buzz",
            contentSuffix,
          });
          window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
            queryKey: ["crew-wiki-events"],
          });
        },
        { owner: OWNER, contentSuffix: LONG_WIKI_BODY },
      );
      await expect
        .poll(() => scroll.evaluate((element) => element.scrollTop), {
          timeout: 2_000,
        })
        .toBe(targetScroll);

      await page.getByRole("button", { name: "Back to wiki library" }).click();
      await expect(page.getByTestId("wiki-library")).toBeVisible();
      await page.evaluate((owner) => {
        window.__BUZZ_E2E_SEED_WIKI__?.({ owner, repoD: "buzz" });
        window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
          queryKey: ["crew-wiki-events"],
        });
      }, OWNER);
      await page
        .getByTestId("wiki-repo-card-buzz")
        .locator("button")
        .first()
        .click();
      await expect(page.getByTestId("wiki-page")).toBeVisible();
      await expect
        .poll(() => scroll.evaluate((element) => element.scrollTop))
        .toBe(0);
      await scroll.hover();
      await page.mouse.wheel(0, 320);
      const cancelledScroll = await scroll.evaluate(
        (element) => element.scrollTop,
      );
      await page.waitForTimeout(450);
      await page.evaluate(
        ({ owner, contentSuffix }) => {
          window.__BUZZ_E2E_SEED_WIKI__?.({
            owner,
            repoD: "buzz",
            contentSuffix,
          });
          window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
            queryKey: ["crew-wiki-events"],
          });
        },
        { owner: OWNER, contentSuffix: LONG_WIKI_BODY },
      );
      await expect
        .poll(() => scroll.evaluate((element) => element.scrollTop))
        .toBe(cancelledScroll);
    });
  });

  test.describe("800x700", () => {
    test.use({ viewport: { width: 800, height: 700 } });

    test("keeps the compact reading surface keyboard reachable", async ({
      page,
    }) => {
      await openSeededRepoWiki(page);

      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      );
      expect(overflow).toBe(true);

      const compactToc = page.getByTestId("wiki-toc-menu");
      if (await compactToc.isVisible()) {
        await compactToc.focus();
        await compactToc.press("Enter");
        const menuItem = page.getByRole("menuitem", {
          name: "Platform Overview",
        });
        await expect(menuItem).toBeVisible();
        await menuItem.focus();
        await menuItem.press("Enter");
      } else {
        const tocItem = page.getByTestId("wiki-toc-overview");
        await expect(tocItem).toBeVisible();
        await tocItem.focus();
        await tocItem.press("Enter");
      }
      await expect(page.getByTestId("wiki-markdown")).toContainText(
        "verified snapshot",
      );

      const search = page.getByTestId("wiki-page-search");
      await search.focus();
      await search.press("ControlOrMeta+A");
      await search.pressSequentially("body search proof");
      const result = page.getByTestId("wiki-search-result-overview");
      await expect(result).toBeVisible();
      await result.focus();
      await result.press("Enter");
      await expect(search).toHaveValue("");

      await openSourceWithKeyboard(page);
      await waitForAnimations(page);
      await page
        .getByTestId("wiki-page")
        .screenshot({ path: `${SHOTS}/02-800x700-keyboard-source.png` });
    });
  });
});
