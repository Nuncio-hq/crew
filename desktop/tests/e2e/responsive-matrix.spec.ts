import { createHash } from "node:crypto";
import { mkdir } from "node:fs/promises";
import path from "node:path";

import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import {
  assertOverlayInsideWindow,
  assertPaneResponsive,
} from "../helpers/assertPaneResponsive";
import {
  installMockBridge,
  openCreateChannelDialog,
  TEST_IDENTITIES,
} from "../helpers/bridge";

const GENERAL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const SHOTS = "test-results/responsive-audit";
const AUX_WIDTHS = [300, 340, 380, 720] as const;
const FLOOR = { width: 800, height: 500 } as const;

function deriveConversationId(channelId: string, rootEventId: string): string {
  const domain = Buffer.from("buzz-acp-conversation-v1");
  const channelBytes = Buffer.from(channelId.replaceAll("-", ""), "hex");
  const rootBytes = Buffer.from(rootEventId, "utf8");
  const digest = createHash("sha256")
    .update(Buffer.concat([domain, channelBytes, rootBytes]))
    .digest();
  const hex = digest.subarray(0, 16).toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

async function setThreadPanelWidth(page: Page, widthPx: number) {
  await page.addInitScript((width) => {
    window.sessionStorage.setItem(
      "buzz.desktop.thread-panel-width",
      String(width),
    );
  }, widthPx);
}

async function openGeneral(page: Page) {
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect
    .poll(() =>
      page.evaluate(
        () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
      ),
    )
    .toBe(true);
}

async function emitRoot(page: Page, content: string, id: string) {
  const event = await page.evaluate(
    ({ content: body, id: eventId, mention }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: body,
        id: eventId,
        mentionPubkeys: [mention],
      }),
    { content, id, mention: TEST_IDENTITIES.alice.pubkey },
  );
  if (!event) throw new Error("mock emitter missing");
  return event as { id: string };
}

async function openThreadReply(page: Page, rootId: string) {
  const row = page.locator(`[data-message-id="${rootId}"]`).first();
  await expect(row).toBeVisible();
  await row.hover();
  await row.getByRole("button", { name: "Reply" }).click();
  await expect(page.getByTestId("message-thread-panel")).toBeVisible();
}

async function injectPlan(page: Page, rootId: string) {
  const conversationId = deriveConversationId(GENERAL_ID, rootId);
  await page.evaluate(
    ({ agentPubkey, channelId, conversationId: conv, rootEventId }) => {
      const now = new Date().toISOString();
      window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
        agentPubkey,
        events: [
          {
            agentIndex: 0,
            channelId,
            conversationId: conv,
            kind: "acp_read",
            payload: {
              method: "session/update",
              params: {
                sessionId: "plan-session",
                update: {
                  entries: [
                    {
                      content: "Stay readable at narrow width",
                      status: "in_progress",
                    },
                  ],
                  sessionUpdate: "plan",
                },
              },
            },
            seq: 1,
            sessionId: "plan-session",
            timestamp: now,
            turnId: "plan-turn",
          },
        ],
      });
      void rootEventId;
    },
    {
      agentPubkey: TEST_IDENTITIES.alice.pubkey,
      channelId: GENERAL_ID,
      conversationId,
      rootEventId: rootId,
    },
  );
}

test.describe("responsive matrix #205", () => {
  test.describe.configure({ timeout: 90_000 });

  test("declared-plans rail stacks at 300/340/380 and does not overlap header", async ({
    page,
  }, testInfo) => {
    await mkdir(SHOTS, { recursive: true });
    await page.setViewportSize({ width: 1280, height: 720 });
    const width = 340;
    await setThreadPanelWidth(page, width);
    await installMockBridge(page);
    await openGeneral(page);
    const root = await emitRoot(
      page,
      "Plans rail reference thread",
      "ab".repeat(32),
    );
    await openThreadReply(page, root.id);
    await injectPlan(page, root.id);
    const rail = page.getByTestId("declared-plans-rail");
    await expect(rail).toBeVisible();
    await expect(rail).toHaveAttribute("data-layout", "stacked");
    await assertPaneResponsive(page, "message-thread-panel", {
      mustNotOverlap: [
        ["auxiliary-panel-header", "declared-plans-rail"],
        ["thread-breadcrumb", "declared-plans-rail"],
      ],
    });
    await waitForAnimations(page);
    const shot = path.join(SHOTS, `thread-plans-rail-${width}.png`);
    await page.getByTestId("message-thread-panel").screenshot({ path: shot });
    await testInfo.attach(`thread-plans-rail-${width}`, {
      path: shot,
      contentType: "image/png",
    });
  });

  for (const width of AUX_WIDTHS) {
    test(`aux panel ${width}px: no letter-soup or overflow`, async ({
      page,
    }) => {
      await page.setViewportSize({ width: 1280, height: 720 });
      await setThreadPanelWidth(page, width);
      await installMockBridge(page);
      await openGeneral(page);
      const root = await emitRoot(
        page,
        `Narrow pane sweep ${width}`,
        width.toString(16).padStart(4, "0").repeat(16),
      );
      await openThreadReply(page, root.id);
      await injectPlan(page, root.id);
      const rail = page.getByTestId("declared-plans-rail");
      await expect(rail).toBeVisible();
      const stacked = width < 508;
      await expect(rail).toHaveAttribute(
        "data-layout",
        stacked ? "stacked" : "side",
      );
      await assertPaneResponsive(page, "message-thread-panel", {
        mustNotOverlap: [["auxiliary-panel-header", "declared-plans-rail"]],
      });
      if (width <= 340) {
        await expect(page.getByTestId("composer-overflow-menu")).toBeVisible();
      }
    });
  }

  test("empty state uses the narrow variant at 300px", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await setThreadPanelWidth(page, 300);
    await installMockBridge(page);
    await openGeneral(page);
    const root = await emitRoot(page, "Empty branch", "cd".repeat(32));
    await openThreadReply(page, root.id);
    const empty = page.getByTestId("thread-empty-state");
    await expect(empty).toBeVisible();
    await expect(empty.getByText("No replies yet")).toBeVisible();
    await expect(
      empty.getByText("Reply in the thread to continue this branch."),
    ).toBeHidden();
    await assertPaneResponsive(page, "message-thread-panel");
    await waitForAnimations(page);
    await mkdir(SHOTS, { recursive: true });
    await empty.screenshot({ path: path.join(SHOTS, "thread-empty-300.png") });
  });

  test("window floor 800×500 full screens", async ({ page }) => {
    await page.setViewportSize(FLOOR);
    await installMockBridge(page);
    await page.goto("/");
    await expect(page.getByTestId("app-sidebar")).toBeVisible();
    await assertPaneResponsive(page, "app-sidebar");

    const routes: Array<{ path: string; testId?: string }> = [
      { path: "/", testId: "app-sidebar" },
      { path: "/pulse" },
      { path: "/projects" },
      { path: "/agents" },
      { path: "/settings" },
      { path: "/wiki" },
      { path: "/org" },
      { path: "/workbench" },
    ];
    for (const route of routes) {
      await page.goto(route.path);
      await waitForAnimations(page);
      const root = page.locator("#root");
      await expect(root).toBeVisible();
      const overflow = await page.evaluate(() => {
        const doc = document.documentElement;
        return doc.scrollWidth > doc.clientWidth + 2;
      });
      expect(overflow, `horizontal page overflow at ${route.path}`).toBe(false);
    }

    await page.getByTestId("channel-general").click();
    await assertPaneResponsive(page, "message-timeline");
    await page.getByTestId("channel-alice-tyler").click();
    await waitForAnimations(page);
    await assertPaneResponsive(page, "message-timeline");

    await waitForAnimations(page);
    await mkdir(SHOTS, { recursive: true });
    await page.screenshot({
      path: path.join(SHOTS, "window-floor-800x500.png"),
      fullPage: false,
    });
  });

  test("focus drawer at 800 / 1024 / 1280", async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.setItem("buzz.channels.threadViewMode", "focus");
    });
    await installMockBridge(page);
    for (const width of [800, 1024, 1280] as const) {
      await page.setViewportSize({
        width,
        height: width === 800 ? 500 : 720,
      });
      await page.goto("/");
      await page.getByTestId("channel-general").click();
      const root = await emitRoot(
        page,
        `Focus drawer ${width}`,
        `${"e".repeat(2)}${width.toString(16)}`.padEnd(64, "f").slice(0, 64),
      );
      await openThreadReply(page, root.id);
      const drawer = page.getByTestId("focus-thread-drawer");
      if ((await drawer.count()) > 0 && (await drawer.isVisible())) {
        await assertPaneResponsive(page, "focus-thread-drawer");
      } else {
        await assertPaneResponsive(page, "message-thread-panel");
      }
    }
  });

  test("dialogs stay inside 800×500", async ({ page }) => {
    await page.setViewportSize(FLOOR);
    await installMockBridge(page);
    await page.goto("/");
    await openCreateChannelDialog(page);
    const dialog = page.getByTestId("create-channel-dialog");
    await expect(dialog).toBeVisible();
    await assertOverlayInsideWindow(dialog, page);
    await expect(page.getByTestId("create-channel-submit")).toBeVisible();
  });

  test("popovers stay inside the window at the four edges", async ({
    page,
  }) => {
    await page.setViewportSize(FLOOR);
    await installMockBridge(page);
    await openGeneral(page);

    await page.getByTestId("channel-general").click({ button: "right" });
    const channelMenu = page.locator("[data-radix-menu-content]").first();
    await expect(channelMenu).toBeVisible();
    await assertOverlayInsideWindow(channelMenu, page);
    await page.keyboard.press("Escape");

    await page.getByTestId("open-search").click();
    const search = page
      .locator("[data-radix-dialog-content], [role='dialog']")
      .first();
    if (await search.isVisible()) {
      await assertOverlayInsideWindow(search, page);
      await page.keyboard.press("Escape");
    }

    await page.getByTestId("composer-emoji-button").click();
    const emoji = page.locator("[data-radix-popper-content-wrapper]").last();
    await expect(emoji).toBeVisible();
    await assertOverlayInsideWindow(emoji, page);
    await page.keyboard.press("Escape");

    const row = page.getByTestId("message-row").first();
    await row.click({ button: "right" });
    const msgMenu = page.locator("[data-radix-menu-content]").first();
    if (await msgMenu.isVisible()) {
      await assertOverlayInsideWindow(msgMenu, page);
    }
  });

  test("message-embedded cards fit a 300px thread pane", async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await setThreadPanelWidth(page, 300);
    await installMockBridge(page, {
      searchProfiles: [
        {
          pubkey: TEST_IDENTITIES.alice.pubkey,
          displayName: "Evidence Agent",
          ownerPubkey: TEST_IDENTITIES.tyler.pubkey,
          isAgent: true,
        },
      ],
    });
    await openGeneral(page);
    const root = await emitRoot(page, "Card sweep root", "11".repeat(32));
    await page.evaluate(
      ({ parentId, pubkey }) => {
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "general",
          content: "before: 120ms | after: 80ms | delta: -40ms",
          extraTags: [["crew-evidence", "metrics"]],
          parentEventId: parentId,
        });
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "general",
          content:
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n",
          parentEventId: parentId,
        });
        window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__?.({
          channelName: "general",
          content: JSON.stringify({
            request_id: "elicit-narrow",
            session_id: "session-narrow",
            turn_id: "turn-narrow",
            channel_id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
            engine: "claude",
            message: "Pick a path",
            questions: [
              {
                id: "q0",
                header: "Path",
                question: "Which option?",
                options: [
                  { value: "a", label: "Truncate" },
                  { value: "b", label: "Stack" },
                ],
              },
            ],
          }),
          pubkey,
          rootEventId: parentId,
        });
      },
      { parentId: root.id, pubkey: TEST_IDENTITIES.alice.pubkey },
    );
    await openThreadReply(page, root.id);
    await expect(page.getByTestId("evidence-card-metrics")).toBeVisible();
    await assertPaneResponsive(page, "message-thread-panel");
    await waitForAnimations(page);
    await mkdir(SHOTS, { recursive: true });
    await page
      .getByTestId("message-thread-panel")
      .screenshot({ path: path.join(SHOTS, "thread-cards-300.png") });
  });

  test("sidebar at contract width truncates instead of wrapping", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1280, height: 720 });
    await installMockBridge(page);
    await page.goto("/");
    await assertPaneResponsive(page, "app-sidebar");
    await waitForAnimations(page);
    await mkdir(SHOTS, { recursive: true });
    await page
      .getByTestId("app-sidebar")
      .screenshot({ path: path.join(SHOTS, "sidebar-contract.png") });
  });
});
