import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const SHOTS = "test-results/crew-wiki";
/** Mock `buzz` card owner (`deadbeef…`), not tyler. Jobs key on this pubkey. */
const OWNER = "deadbeef".repeat(8);

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

async function openWiki(page: import("@playwright/test").Page) {
  await page.getByTestId("open-wiki-view").click();
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
    await expect(page.getByTestId("open-wiki-view")).toBeVisible();

    await openWiki(page);
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
    await page
      .getByTestId("wiki-ask")
      .getByRole("button", { name: "Ask" })
      .click();
    await expect(page.getByTestId("wiki-ask-answer")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-ask")
      .screenshot({ path: `${SHOTS}/08-ask-qa.png` });

    await page.getByTestId("wiki-ask-mode").selectOption("plan");
    await page
      .getByTestId("wiki-ask-input")
      .fill("How should we document the relay?");
    await page
      .getByTestId("wiki-ask")
      .getByRole("button", { name: "Ask" })
      .click();
    await expect(page.getByTestId("wiki-start-thread")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-ask")
      .screenshot({ path: `${SHOTS}/09-ask-plan.png` });

    await page
      .getByTestId("wiki-markdown")
      .getByRole("button", { name: /Open .*ProjectDetailScreen/ })
      .click();
    await expect(page).toHaveURL(/#\/projects\//);
    await expect(page.getByTestId("wiki-file-panel")).toBeVisible();
    await expect(page.getByTestId("wiki-file-highlight").first()).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-file-panel")
      .screenshot({ path: `${SHOTS}/10-file-citation.png` });

    const filePanel = page.getByTestId("wiki-file-panel");
    await expect(
      filePanel.locator('[data-testid="wiki-file-highlight"]'),
    ).toHaveCount(3);
    await page.evaluate(
      (href) =>
        window.__TAURI_INTERNALS__?.invoke?.("plugin:event|emit", {
          event: "deep-link-entity",
          payload: href,
        }),
      `buzz://file?owner=${OWNER}&d=buzz&path=desktop/src/features/projects/ui/ProjectDetailScreen.tsx&lines=2-3`,
    );
    await expect
      .poll(() =>
        filePanel
          .getByTestId("wiki-file-highlight")
          .evaluateAll((lines) =>
            lines.map((line) => line.getAttribute("data-line")),
          ),
      )
      .toEqual(["2", "3"]);
    await expect(filePanel.locator('[data-line="1"]')).not.toHaveAttribute(
      "data-testid",
      "wiki-file-highlight",
    );

    await page.getByTestId("project-wiki-tab").click();
    await expect(page.getByTestId("wiki-project-tab")).toBeVisible();
    await expect(page.getByTestId("wiki-cadence")).toHaveCount(0);
    await expect(page.getByTestId("wiki-generate-mirror")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("wiki-project-tab")
      .screenshot({ path: `${SHOTS}/11-project-wiki-tab.png` });

    await openWiki(page);
    await page.getByTestId("wiki-company-card").click();
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
    await page
      .getByTestId("wiki-ask")
      .getByRole("button", { name: "Ask" })
      .click();
    await page.getByTestId("wiki-plan-channel").selectOption({ index: 1 });
    await page.getByTestId("wiki-start-thread").click();
    await expect(page).toHaveURL(/#\/channels\//);
    await expect(page.locator(".ProseMirror").first()).toContainText("Plan");
    await waitForAnimations(page);
    await page
      .locator(".ProseMirror")
      .first()
      .screenshot({ path: `${SHOTS}/14-plan-thread.png` });
  });
});
