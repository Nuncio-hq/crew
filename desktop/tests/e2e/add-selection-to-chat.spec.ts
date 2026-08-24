import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { waitForAnimations } from "../helpers/animations";

const AGENT_PUBKEY =
  "a1b2c3d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccddeeff00";

async function waitForMockLiveSubscription(
  page: import("@playwright/test").Page,
) {
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
}

test("adds selected agent text to the current composer", async ({ page }) => {
  await installMockBridge(page);
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForMockLiveSubscription(page);
  await page.evaluate(
    ({ pubkey }) => {
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: "The release gate needs one final accessibility review.",
        pubkey,
      });
    },
    { pubkey: AGENT_PUBKEY },
  );

  const messageText = page.getByText(
    "The release gate needs one final accessibility review.",
    { exact: true },
  );
  await expect(messageText).toBeVisible();

  const editor = page
    .getByTestId("channel-composer-overlay")
    .locator("[contenteditable='true']");
  await editor.fill("Can you expand on this?");

  await messageText.evaluate((element) => {
    const textNode = element.firstChild;
    if (!textNode) throw new Error("message text node missing");
    const text = textNode.textContent ?? "";
    const start = text.indexOf("one final accessibility review");
    const range = document.createRange();
    range.setStart(textNode, start);
    range.setEnd(textNode, start + "one final accessibility review".length);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    element.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
  });

  const addButton = page.getByTestId("add-selection-to-chat");
  await expect(addButton).toBeVisible();
  await waitForAnimations(page);
  await page.screenshot({
    path: "test-results/add-selection-to-chat-action.png",
  });
  await addButton.click();

  await expect(editor.locator("blockquote")).toContainText(
    "one final accessibility review",
  );
  await expect(editor).toContainText("Can you expand on this?");
  await expect(editor).toBeFocused();
  await waitForAnimations(page);
  await page.screenshot({
    path: "test-results/add-selection-to-chat-composer.png",
  });
});
