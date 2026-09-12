import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const SHOTS = "test-results/crew-wiki";
/** Mock `buzz` card owner (`deadbeef…`), not tyler. Jobs key on this pubkey. */
const OWNER = "deadbeef".repeat(8);
const SOURCE_PATH = "desktop/src/features/projects/ui/ProjectDetailScreen.tsx";

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

async function openWiki(page: import("@playwright/test").Page) {
  await page.getByTestId("workspace-menu-trigger").click();
  await page.getByTestId("workspace-company-wiki").click();
  await expect(page).toHaveURL(/#\/wiki$/);
  await expect(page.getByTestId("wiki-library")).toBeVisible();
  await expect(page.getByText("Create company page")).toHaveCount(0);
  await expect(page.getByTestId("wiki-home-search")).toBeVisible();
}

test.describe("Crew Wiki (#200)", () => {
  test("library states, page, mermaid, ask, plan, project tab, company review", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/");
    await expect(page.getByTestId("workspace-menu-trigger")).toBeVisible();

    await openWiki(page);
    await expect(page.getByTestId("wiki-recovery-error")).toHaveCount(0);
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-library")
      .screenshot({ path: `${SHOTS}/01-library-never.png` });

    await page.evaluate((owner) => {
      window.__BUZZ_E2E_SET_WIKI_JOB__?.({
        repoKey: `${owner}:buzz`.toLowerCase(),
        status: "generating",
        done: 1,
        total: 4,
        error: null,
        costNote: "Heuristic generator · no API key billed",
      });
    }, OWNER);
    await expect(page.getByTestId("wiki-generating")).toBeVisible();
    await expect(page.getByTestId("wiki-repo-card-relay-tools")).toHaveCount(1);
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-repo-card-buzz")
      .screenshot({ path: `${SHOTS}/02-library-generating.png` });

    await page.evaluate((owner) => {
      window.__BUZZ_E2E_SET_WIKI_JOB__?.({
        repoKey: `${owner}:buzz`.toLowerCase(),
        status: "idle",
        done: 1,
        total: 1,
        error: null,
        costNote: null,
      });
      window.__BUZZ_E2E_SEED_WIKI__?.({ owner, repoD: "buzz" });
      window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
        queryKey: ["crew-wiki-events"],
      });
    }, OWNER);
    await expect(page.getByTestId("wiki-repo-card-buzz")).toContainText(
      "minutes",
    );
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-repo-card-buzz")
      .screenshot({ path: `${SHOTS}/03-library-fresh.png` });

    await page.evaluate((owner) => {
      window.__BUZZ_E2E_SEED_WIKI__?.({
        owner,
        repoD: "buzz",
        commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
      });
      window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
        queryKey: ["crew-wiki-events"],
      });
    }, OWNER);
    await expect(page.getByTestId("wiki-regenerate-buzz")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-repo-card-buzz")
      .screenshot({ path: `${SHOTS}/04-library-stale.png` });

    await page.evaluate((owner) => {
      window.__BUZZ_E2E_SET_WIKI_JOB__?.({
        repoKey: `${owner}:buzz`.toLowerCase(),
        status: "failed",
        done: 0,
        total: 4,
        error: "generator timed out",
        costNote: "Heuristic generator · no API key billed",
      });
    }, OWNER);
    await expect(page.getByTestId("wiki-failed")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-repo-card-buzz")
      .screenshot({ path: `${SHOTS}/05-library-failed.png` });

    await page.evaluate((owner) => {
      window.__BUZZ_E2E_SET_WIKI_JOB__?.({
        repoKey: `${owner}:buzz`.toLowerCase(),
        status: "idle",
        done: 1,
        total: 1,
        error: null,
        costNote: null,
      });
      window.__BUZZ_E2E_SEED_WIKI__?.({ owner, repoD: "buzz" });
      window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
        queryKey: ["crew-wiki-events"],
      });
    }, OWNER);
    await expect(page.getByTestId("wiki-repo-card-buzz")).toContainText(
      "minutes",
    );

    await page
      .getByTestId("wiki-repo-card-buzz")
      .locator("button")
      .first()
      .click();
    await expect(page.getByTestId("wiki-page")).toBeVisible();
    await expect(page.getByTestId("wiki-toc")).toBeVisible();
    await expect(page.getByTestId("wiki-toc-overview")).toBeVisible();
    await page.getByTestId("wiki-toc-overview").click();
    await expect(page.getByTestId("wiki-cadence")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-page")
      .screenshot({ path: `${SHOTS}/06-wiki-page.png` });

    const pageSearch = page.getByTestId("wiki-page-search");
    await pageSearch.fill("body search proof");
    await expect(page.getByTestId("wiki-search-results")).toBeVisible();
    await expect(page.getByTestId("wiki-search-result-overview")).toContainText(
      "body search proof",
    );
    await page.getByTestId("wiki-search-result-overview").click();
    await expect(pageSearch).toHaveValue("");
    await expect(page.getByTestId("wiki-markdown")).toContainText(
      "body search proof",
    );

    const sourceFiles = page.getByTestId("wiki-source-files");
    await expect(sourceFiles).toBeVisible();
    await sourceFiles.locator("summary").click();
    await expect(
      sourceFiles.getByText("Folder: E2E mock Buzz checkout"),
    ).toBeVisible();
    await sourceFiles.getByRole("button", { name: SOURCE_PATH }).click();
    await expect(page.getByTestId("wiki-source-pane")).toBeVisible();
    await expect(page.getByTestId("wiki-toc")).toHaveCount(0);
    await expect(page.getByTestId("wiki-source-preview")).toContainText(
      "export function ProjectDetailScreen()",
    );
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("wiki-source-pane")).toHaveCount(0);
    await expect(
      sourceFiles.getByRole("button", { name: SOURCE_PATH }),
    ).toBeFocused();

    await expect(page.getByTestId("wiki-mermaid")).toBeVisible();
    await expect(page.getByTestId("wiki-mermaid-fallback")).toBeVisible();
    await page.getByTestId("wiki-mermaid").click();
    await expect(page.getByTestId("wiki-mermaid-lightbox")).toBeVisible();
    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/07-mermaid-lightbox.png`,
      clip: { x: 200, y: 40, width: 880, height: 520 },
    });
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("wiki-mermaid-lightbox")).toHaveCount(0);

    await page.getByTestId("wiki-ask-mode").selectOption("qa");
    await page.getByTestId("wiki-ask-input").fill("What is Crew Wiki?");
    await expect(page.getByTestId("wiki-ask-unavailable")).toBeVisible();
    await expect(
      page.getByTestId("wiki-ask").getByRole("button", { name: "Ask" }),
    ).toBeDisabled();
    await expect(page.getByTestId("wiki-ask-answer")).toHaveCount(0);
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-ask")
      .screenshot({ path: `${SHOTS}/08-ask-unavailable-qa.png` });

    await page.getByTestId("wiki-ask-mode").selectOption("plan");
    await page
      .getByTestId("wiki-ask-input")
      .fill("How should we document the relay?");
    await page.getByTestId("wiki-ask-input").press("Enter");
    await expect(page.getByTestId("wiki-ask-unavailable")).toBeVisible();
    await expect(page.getByTestId("wiki-start-thread")).toHaveCount(0);
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-ask")
      .screenshot({ path: `${SHOTS}/09-ask-unavailable-plan.png` });

    await page
      .getByTestId("wiki-markdown")
      .getByRole("button", { name: /Open .*ProjectDetailScreen/ })
      .click();
    await expect(page).toHaveURL(/#\/wiki/);
    await expect(page.getByTestId("wiki-source-pane")).toBeVisible();
    await expect(page.getByTestId("wiki-source-preview")).toContainText(
      "export function ProjectDetailScreen()",
    );
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-source-pane")
      .screenshot({ path: `${SHOTS}/10-file-citation.png` });
    await page.getByTestId("wiki-source-pane-close").click();
    await expect(page.getByTestId("wiki-source-pane")).toHaveCount(0);

    await expect(
      page.getByRole("button", { name: "Open project" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Open project" }).click();
    const buzzWorkspaceCard = page.locator("article").filter({
      has: page.getByRole("heading", { name: "buzz", exact: true }),
    });
    await expect(buzzWorkspaceCard).toBeVisible();
    await buzzWorkspaceCard
      .getByRole("button", { name: "Open workspace details" })
      .click();
    await expect(page.getByTestId("project-wiki-tab")).toBeVisible();
    await page.getByTestId("project-wiki-tab").click();
    await expect(page.getByTestId("wiki-project-tab")).toBeVisible();
    await expect(page.getByTestId("wiki-cadence")).toHaveCount(0);
    await expect(page.getByTestId("wiki-generate-mirror")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-project-tab")
      .screenshot({ path: `${SHOTS}/11-project-wiki-tab.png` });

    const projectDetailScroll = page.getByTestId("project-detail-scroll");
    const projectTabGeometry = await page.evaluate(() => {
      const tab = document.querySelector<HTMLElement>(
        '[data-testid="project-wiki-tab"]',
      );
      const header = document.querySelector<HTMLElement>(
        '[data-testid="wiki-header-bar"]',
      );
      if (!tab || !header) {
        throw new Error("Project Wiki tab geometry targets are missing");
      }
      return {
        scrollTop: document.querySelector<HTMLElement>(
          '[data-testid="project-detail-scroll"]',
        )?.scrollTop,
        tabTop: tab.getBoundingClientRect().top,
        headerTop: header.getBoundingClientRect().top,
      };
    });
    await projectDetailScroll.evaluate(
      (element, scrollTop) => {
        element.scrollTop = Math.max(1, Math.ceil(scrollTop));
        element.dispatchEvent(new Event("scroll", { bubbles: true }));
      },
      (projectTabGeometry.scrollTop ?? 0) +
        projectTabGeometry.headerTop -
        projectTabGeometry.tabTop,
    );
    await expect
      .poll(() => projectDetailScroll.evaluate((element) => element.scrollTop))
      .toBeGreaterThan(0);
    const overlap = await page.evaluate(() => {
      const tab = document.querySelector<HTMLElement>(
        '[data-testid="project-wiki-tab"]',
      );
      const header = document.querySelector<HTMLElement>(
        '[data-testid="wiki-header-bar"]',
      );
      if (!tab || !header) {
        throw new Error("Project Wiki tab geometry targets are missing");
      }
      const tabRect = tab.getBoundingClientRect();
      const headerRect = header.getBoundingClientRect();
      const point = {
        x: tabRect.left + tabRect.width / 2,
        y: tabRect.top + tabRect.height / 2,
      };
      const hit = document.elementFromPoint(point.x, point.y);
      return {
        tabRect,
        headerRect,
        hitTestId: hit?.closest<HTMLElement>("[data-testid]")?.dataset.testid,
      };
    });
    expect(overlap.headerRect.top).toBeLessThan(overlap.tabRect.bottom);
    expect(overlap.headerRect.bottom).toBeGreaterThan(overlap.tabRect.top);
    expect(overlap.hitTestId).toBe("project-wiki-tab");
    await waitForAnimations(page);
    await projectDetailScroll.screenshot({
      path: `${SHOTS}/15-project-tab-outer-scroll.png`,
    });

    await openWiki(page);
    await page.getByTestId("wiki-company-card").getByRole("button").click();
    await expect(page.getByTestId("wiki-company-empty")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-company-empty")
      .screenshot({ path: `${SHOTS}/12-company-empty.png` });

    await page.evaluate((pubkey) => {
      window.__BUZZ_E2E_SEED_COMPANY_WIKI__?.({ pubkey, proposal: true });
      window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
        queryKey: ["crew-wiki-events"],
      });
    }, OWNER);
    await expect(page.getByTestId("wiki-proposal-queue")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-proposal-queue")
      .screenshot({ path: `${SHOTS}/13-company-proposal.png` });

    await page.getByTestId("wiki-proposal-accept-engram-note").click();
    await expect(
      page.getByTestId("wiki-proposal-accept-engram-note"),
    ).toHaveCount(0);

    await page.getByTestId("wiki-ask-mode").selectOption("plan");
    await page.getByTestId("wiki-ask-input").fill("plan a company wiki pass");
    await page.getByTestId("wiki-ask-input").press("Enter");
    await expect(page.getByTestId("wiki-ask-unavailable")).toBeVisible();
    await expect(page.getByTestId("wiki-start-thread")).toHaveCount(0);
    await expect(page).toHaveURL(/#\/wiki$/);
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-ask")
      .screenshot({ path: `${SHOTS}/14-company-ask-unavailable.png` });
  });
});
