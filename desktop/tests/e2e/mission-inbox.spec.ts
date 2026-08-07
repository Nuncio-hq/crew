import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const SHOTS = "test-results/mission-inbox";
const CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const ROOT_ID = "1".repeat(64);
const REQUEST_ID = ROOT_ID;
const CONVERSATION_ID = "2096b1ca-3834-7197-6a2a-bc5b580e07e6";

const MOCK_PUBKEY = "deadbeef".repeat(8);

test.describe("mission inbox", () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test("ingests a live 46040 request, falls back to its channel, and resolves it", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-inbox-list")).toBeVisible({
      timeout: 10_000,
    });

    await expect
      .poll(() =>
        page.evaluate(
          ({ channelName }) =>
            window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
              channelName,
              kind: 46040,
            }) ?? false,
          { channelName: "general" },
        ),
      )
      .toBe(true);
    await page.evaluate(
      ({ channelId, id, pubkey }) => {
        const emit = window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__;
        if (!emit) throw new Error("Mock user-input helper is unavailable.");
        emit({
          channelName: "general",
          requestId: id,
          pubkey,
          content: JSON.stringify({
            channel_id: channelId,
            engine: "codex",
            message: "Approval is waiting for you",
            questions: [],
            request_id: id,
            session_id: "mission-session",
            turn_id: "mission-turn",
          }),
        });
      },
      { channelId: CHANNEL_ID, id: REQUEST_ID, pubkey: MOCK_PUBKEY },
    );

    const sections = page.getByTestId("mission-inbox-sections");
    await expect(sections).toBeVisible();
    await expect(
      page.getByTestId("mission-inbox-section-needsYou"),
    ).toContainText("Needs you");
    await expect(
      page.getByTestId("mission-inbox-section-readyToReview"),
    ).toContainText("Ready to review");
    await expect(
      page.getByTestId("mission-inbox-section-working"),
    ).toContainText("In flight");
    await expect(
      page.getByTestId(`mission-inbox-row-${CONVERSATION_ID}`),
    ).toBeVisible();

    await waitForAnimations(page);
    await sections.screenshot({ path: `${SHOTS}/01-sections.png` });

    const urlBefore = page.url();
    await page.getByTestId(`mission-inbox-row-${CONVERSATION_ID}`).click();
    await expect.poll(() => page.url()).not.toBe(urlBefore);
    await expect(page).toHaveURL(new RegExp(`/channels/${CHANNEL_ID}`));

    await waitForAnimations(page);
    await page.screenshot({ path: `${SHOTS}/02-channel-fallback.png` });

    await page.evaluate(
      ({ requestEventId }) =>
        window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT_ANSWER__?.({
          channelName: "general",
          requestEventId,
        }),
      { requestEventId: REQUEST_ID },
    );
    await expect(
      page.getByTestId(`mission-inbox-row-${CONVERSATION_ID}`),
    ).toHaveCount(0);
  });
});
