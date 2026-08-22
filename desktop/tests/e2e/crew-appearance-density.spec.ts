import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

const SHOTS = "test-results/crew-appearance";
const DENSITY_STORAGE_KEY = "buzz.appearance.conversationDensity";
const FONT_SIZE_STORAGE_KEY = "buzz.appearance.fontSize";

async function openGeneral(page: Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("app-sidebar")).toBeVisible();
}

type ConversationMetrics = {
  bodyGapPx: number;
  rowPaddingPx: number;
};

async function messageBodyFontSizePx(page: Page) {
  await expect(
    page.locator("[data-testid='message-row'] .message-markdown").first(),
  ).toBeVisible();

  return page.evaluate(() => {
    const body = document.querySelector(
      "[data-testid='message-row'] .message-markdown",
    );
    if (!body) {
      throw new Error("no message body rendered");
    }

    return Number.parseFloat(getComputedStyle(body).fontSize);
  });
}

async function conversationMetrics(page: Page): Promise<ConversationMetrics> {
  await expect(
    page.locator("[data-testid='message-row'] .message-markdown").first(),
  ).toBeVisible();

  return page.evaluate(() => {
    const row = document.querySelector("[data-testid='message-row']");
    const body = row?.querySelector(".message-markdown")?.parentElement;
    if (!row || !body) {
      throw new Error("no message row rendered");
    }

    return {
      bodyGapPx: Number.parseFloat(getComputedStyle(body).marginTop),
      rowPaddingPx: Number.parseFloat(getComputedStyle(row).paddingTop),
    };
  });
}

test("appearance settings expose font size and density with a Crew-themed preview", async ({
  page,
}) => {
  await installMockBridge(page);
  await page.goto("/");
  await openSettings(page, "appearance");
  await waitForAnimations(page);
  await page.screenshot({
    fullPage: true,
    path: `${SHOTS}/01-appearance-defaults.png`,
  });

  // The appearance panel scrolls internally, so `fullPage` alone cannot reach
  // the display card; scroll the panel itself before the second capture.
  await page.getByTestId("settings-content-scroll").evaluate((element) => {
    element.scrollTo({ top: element.scrollHeight });
  });
  await waitForAnimations(page);
  await page.screenshot({
    fullPage: true,
    path: `${SHOTS}/06-appearance-display.png`,
  });

  await expect(page.getByTestId("font-size-control")).toBeVisible();
  await expect(page.getByTestId("conversation-density-control")).toBeVisible();
  await expect(page.getByTestId("conversation-preview")).toBeVisible();

  // Crew guardrails: chrome stays Crew Dark and the Buzz gradient stays retired.
  await expect(page.locator("html")).toHaveAttribute(
    "data-crew-chrome",
    "crew-dark",
  );
  await expect(page.locator("html")).not.toHaveAttribute("data-buzz-sidebar");
  await expect(page.getByTestId("appearance-syntax-row")).toBeVisible();
});

test("conversation density persists and tightens the message timeline", async ({
  page,
}) => {
  await installMockBridge(page);
  await openGeneral(page);
  await waitForAnimations(page);
  await page.screenshot({
    fullPage: true,
    path: `${SHOTS}/02-timeline-comfortable.png`,
  });

  const comfortable = await conversationMetrics(page);

  await openSettings(page, "appearance");
  await page.getByTestId("conversation-density-compact").click();

  await expect(page.locator("html")).toHaveAttribute(
    "data-conversation-density",
    "compact",
  );
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        DENSITY_STORAGE_KEY,
      ),
    )
    .toBe("compact");

  await page.keyboard.press("Escape");
  await openGeneral(page);
  await expect(page.locator("html")).toHaveAttribute(
    "data-conversation-density",
    "compact",
  );
  const compact = await conversationMetrics(page);
  expect(compact.bodyGapPx).toBeLessThan(comfortable.bodyGapPx);

  await waitForAnimations(page);
  await page.screenshot({
    fullPage: true,
    path: `${SHOTS}/03-timeline-compact.png`,
  });

  await openSettings(page, "appearance");
  await page.getByTestId("conversation-density-spacious").click();
  await page.keyboard.press("Escape");
  await openGeneral(page);

  const spacious = await conversationMetrics(page);
  expect(spacious.rowPaddingPx).toBeGreaterThan(compact.rowPaddingPx);
  expect(spacious.bodyGapPx).toBeGreaterThan(comfortable.bodyGapPx);

  await waitForAnimations(page);
  await page.screenshot({
    fullPage: true,
    path: `${SHOTS}/05-timeline-spacious.png`,
  });
});

test("font size preference scales conversation type app-wide", async ({
  page,
}) => {
  await installMockBridge(page);
  await openGeneral(page);

  const defaultFontSize = await messageBodyFontSizePx(page);

  await openSettings(page, "appearance");
  await page.getByTestId("font-size-larger").click();

  await expect(page.locator("html")).toHaveAttribute(
    "data-font-size",
    "larger",
  );
  await expect
    .poll(() =>
      page.evaluate(
        (key) => window.localStorage.getItem(key),
        FONT_SIZE_STORAGE_KEY,
      ),
    )
    .toBe("larger");

  await page.keyboard.press("Escape");
  await openGeneral(page);

  const largerFontSize = await messageBodyFontSizePx(page);

  expect(largerFontSize).toBeGreaterThan(defaultFontSize);

  await waitForAnimations(page);
  await page.screenshot({
    fullPage: true,
    path: `${SHOTS}/04-timeline-larger-type.png`,
  });
});
