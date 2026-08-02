import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { TEST_IDENTITIES, installMockBridge } from "../helpers/bridge";

const GENERAL_CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";

type MockMessageEvent = {
  id: string;
  created_at: number;
  pubkey: string;
};

async function waitForMockLiveSubscription(
  page: import("@playwright/test").Page,
  channelName: string,
) {
  await expect
    .poll(async () => {
      return page.evaluate(
        ({ ch }) =>
          (
            window as Window & {
              __BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?: (input: {
                channelName: string;
              }) => boolean;
            }
          ).__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({ channelName: ch }) ??
          false,
        { ch: channelName },
      );
    })
    .toBe(true);
}

async function emitMockMessage(
  page: import("@playwright/test").Page,
  channelName: string,
  content: string,
  options?: {
    parentEventId?: string;
    pubkey?: string;
    createdAt?: number;
  },
): Promise<MockMessageEvent> {
  const event = await page.evaluate(
    ({ ch, msg, parentEventId, pubkey, ts }) => {
      return (
        window as Window & {
          __BUZZ_E2E_EMIT_MOCK_MESSAGE__?: (input: {
            channelName: string;
            content: string;
            parentEventId?: string | null;
            pubkey?: string;
            createdAt?: number;
          }) => { id: string; created_at: number; pubkey: string };
        }
      ).__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: ch,
        content: msg,
        parentEventId: parentEventId ?? undefined,
        pubkey: pubkey ?? undefined,
        createdAt: ts,
      });
    },
    {
      ch: channelName,
      msg: content,
      parentEventId: options?.parentEventId ?? null,
      pubkey: options?.pubkey ?? TEST_IDENTITIES.alice.pubkey,
      ts: options?.createdAt,
    },
  );
  if (!event) {
    throw new Error("Mock message emitter is not installed");
  }
  return event;
}

async function openGeneralWithReply(
  page: import("@playwright/test").Page,
  replyBody = "Orientation reply for breadcrumb",
) {
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await waitForMockLiveSubscription(page, "general");

  await emitMockMessage(page, "general", replyBody, {
    parentEventId: "mock-general-welcome",
    pubkey: TEST_IDENTITIES.alice.pubkey,
  });

  const threadSummary = page.getByTestId("message-thread-summary").first();
  await expect(threadSummary).toBeVisible();
  await threadSummary.click();
  await expect(page.getByTestId("message-thread-panel")).toBeVisible();
  await waitForAnimations(page);
}

test.describe("thread orientation", () => {
  test("01-breadcrumb-shows-channel-and-navigates-to-anchor", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/");

    await openGeneralWithReply(page);

    const breadcrumb = page.getByTestId("thread-breadcrumb");
    await expect(breadcrumb).toBeVisible();
    await expect(breadcrumb).toContainText("#general");
    // Head author must appear (welcome message author from mock seed).
    await expect(breadcrumb).not.toHaveText(/^Thread$/);

    const timeline = page.getByTestId("message-timeline");
    // Scroll the channel away from the root so the breadcrumb jump is real.
    await timeline.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });

    await breadcrumb.click();
    await waitForAnimations(page);

    const anchor = page.locator('[data-thread-anchor="true"]');
    await expect(anchor).toBeVisible();
    await expect(anchor).toBeInViewport();
    // Split mode keeps the thread open after breadcrumb navigation.
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
  });

  test("02-timeline-anchor-state-and-viewing-thread-pill", async ({ page }) => {
    await installMockBridge(page);
    await page.goto("/");

    await openGeneralWithReply(page);

    const anchor = page.locator('[data-thread-anchor="true"]');
    await expect(anchor).toHaveCount(1);
    await expect(anchor).toHaveAttribute("aria-current", "location");
    await expect(
      page
        .getByTestId("message-thread-summary")
        .filter({ hasText: "Viewing thread" }),
    ).toBeVisible();

    await page.getByTestId("auxiliary-panel-close").click();
    await expect(page.getByTestId("message-thread-panel")).not.toBeVisible();
    await expect(page.locator('[data-thread-anchor="true"]')).toHaveCount(0);

    // Open a different root so the anchor moves.
    const otherRoot = await emitMockMessage(
      page,
      "general",
      "Second orientation root",
      { pubkey: TEST_IDENTITIES.bob.pubkey },
    );
    await emitMockMessage(page, "general", "Reply under second root", {
      parentEventId: otherRoot.id,
      pubkey: TEST_IDENTITIES.alice.pubkey,
    });

    const secondSummary = page.locator(
      `[data-testid="message-thread-summary"][data-thread-head-id="${otherRoot.id}"]`,
    );
    await expect(secondSummary).toBeVisible();
    await secondSummary.click();
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();

    const anchoredRow = page.locator('[data-thread-anchor="true"]');
    await expect(anchoredRow).toHaveCount(1);
    await expect(
      anchoredRow.locator(`[data-message-id="${otherRoot.id}"]`),
    ).toBeVisible();
  });

  test("03-nested-head-keeps-top-level-anchor-and-shows-ancestry", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/");

    await page.getByTestId("channel-general").click();
    await expect(page.getByTestId("chat-title")).toHaveText("general");
    await waitForMockLiveSubscription(page, "general");

    const past = Math.floor(Date.now() / 1000) - 60;
    const mid = await emitMockMessage(page, "general", "Nested mid parent", {
      parentEventId: "mock-general-welcome",
      pubkey: TEST_IDENTITIES.alice.pubkey,
      createdAt: past,
    });
    await emitMockMessage(page, "general", "Nested leaf under mid", {
      parentEventId: mid.id,
      pubkey: TEST_IDENTITIES.bob.pubkey,
      createdAt: past + 1,
    });

    // Depth-0 open: no ancestry strip.
    const rootSummary = page.getByTestId("message-thread-summary").first();
    await expect(rootSummary).toBeVisible();
    await rootSummary.click();
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
    await expect(page.getByTestId("thread-ancestry-strip")).toHaveCount(0);
    await expect(page.locator('[data-thread-anchor="true"]')).toHaveCount(1);

    // Open the mid reply as its own thread head via URL panel state.
    await page.goto(`/#/channels/${GENERAL_CHANNEL_ID}?thread=${mid.id}`);
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
    await waitForAnimations(page);

    const breadcrumb = page.getByTestId("thread-breadcrumb");
    await expect(breadcrumb).toBeVisible();
    await expect(breadcrumb).toContainText("#general");

    const strip = page.getByTestId("thread-ancestry-strip");
    await expect(strip).toBeVisible();
    await expect(page.getByTestId("thread-ancestry-row").first()).toBeVisible();

    // Anchor stays on the top-level welcome message, not the nested head.
    const anchor = page.locator('[data-thread-anchor="true"]');
    await expect(anchor).toHaveCount(1);
    await expect(
      anchor.locator('[data-message-id="mock-general-welcome"]'),
    ).toBeVisible();

    // Click ancestry row → panel head becomes the parent (welcome); strip gone.
    await page.getByTestId("thread-ancestry-row").first().click();
    await waitForAnimations(page);
    await expect(page.getByTestId("thread-ancestry-strip")).toHaveCount(0);
    await expect(
      page
        .getByTestId("message-thread-head")
        .locator('[data-message-id="mock-general-welcome"]'),
    ).toBeVisible();
  });
});
