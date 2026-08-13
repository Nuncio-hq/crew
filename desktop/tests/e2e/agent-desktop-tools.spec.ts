import { expect, test, type Page } from "@playwright/test";

import type { SimHolding } from "../../src/features/tool-pane/types";
import type { AgentControlUi } from "../../src/features/tool-pane/agentControlStore";
import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";

const GENERAL = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const SHOTS = "test-results/agent-desktop-tools";
const MOCK_ID = "a".repeat(64);

function simHolding(
  patch: Partial<SimHolding> & { channelId: string },
): SimHolding {
  return {
    channelName: patch.channelName ?? "general",
    deviceName: `crew-${patch.channelId.replace(/-/g, "").slice(0, 8)}`,
    udid: patch.udid ?? "UDID-e2e",
    lifecycle: "shutdown",
    deviceType: "iPhone 16 Pro",
    runtime: "iOS 18",
    foreign: false,
    diskBytes: 4.5 * 1024 * 1024 * 1024,
    lastUsedMs: Date.now(),
    idleDeadlineMs: null,
    paneVisible: false,
    mirroring: false,
    lastScreenshotDataUrl: null,
    bootElapsedMs: null,
    ...patch,
  };
}

async function openTools(page: Page, tab: "sim" | "browser" = "sim") {
  await page.getByTestId("tools-header-button").click();
  await page
    .getByTestId(tab === "sim" ? "tools-open-sim" : "tools-open-browser")
    .click();
  await expect(page.getByTestId("channel-tool-pane")).toBeVisible();
}

async function seedControl(page: Page, next: AgentControlUi) {
  await page.evaluate((payload) => {
    window.__BUZZ_E2E_SET_AGENT_CONTROL__?.(payload);
  }, next);
}

test.describe("agent desktop tools (#197)", () => {
  test("snapshot click overlay and take-over banner", async ({ page }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "browser");
    await seedControl(page, {
      leases: [
        {
          channelId: GENERAL,
          instrument: "browser",
          state: "agentHeld",
          agentName: "Hermes",
          humanHeldUntilMs: null,
        },
      ],
      overlay: {
        instrument: "browser",
        tool: "browser_click",
        channelId: GENERAL,
        point: { x: 80, y: 120 },
        atMs: Date.now(),
      },
      pendingOrigin: null,
    });
    await expect(page.getByTestId("browser-driving-banner")).toContainText(
      "Hermes is driving",
    );
    await expect(page.getByTestId("browser-ghost-cursor")).toBeVisible();
    await expect(page.getByTestId("browser-tap-ripple")).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("tool-pane-browser").screenshot({
      path: `${SHOTS}/01-agent-driving-overlay.png`,
    });

    await page.getByTestId("browser-lease-take-over").click();
    await expect(page.getByTestId("browser-driving-banner")).toContainText(
      "You have control",
    );
    await expect(page.getByTestId("browser-lease-release")).toBeVisible();
    await expect(page.getByTestId("browser-lease-take-over")).toHaveCount(0);
    await waitForAnimations(page);
    await page.getByTestId("browser-driving-banner").screenshot({
      path: `${SHOTS}/02-human-take-over.png`,
    });
  });

  test("instrument is not the pane: sidebar dot then mid-flight reveal", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("channel-general")).toBeVisible();
    await seedControl(page, {
      leases: [
        {
          channelId: GENERAL,
          instrument: "sim",
          state: "agentHeld",
          agentName: "Hermes",
          humanHeldUntilMs: null,
        },
      ],
      overlay: {
        instrument: "sim",
        tool: "sim_tap",
        channelId: GENERAL,
        point: { x: 40, y: 80 },
        atMs: Date.now(),
      },
      pendingOrigin: null,
    });
    await expect(page.getByTestId("resource-dot-sim")).toBeVisible();
    await waitForAnimations(page);
    await page.locator("[data-testid='channel-general']").screenshot({
      path: `${SHOTS}/03-sidebar-dot-pane-closed.png`,
    });

    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await expect(page.getByTestId("sim-driving-banner")).toContainText(
      "Hermes is driving",
    );
    await expect(page.getByTestId("sim-ghost-cursor")).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("tool-pane-sim").screenshot({
      path: `${SHOTS}/04-pane-opens-mid-flight.png`,
    });
  });

  test("foreign origin Allow domain writes canvas; Deny is origin_blocked", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "browser");
    await seedControl(page, {
      leases: [
        {
          channelId: GENERAL,
          instrument: "browser",
          state: "agentHeld",
          agentName: "Hermes",
          humanHeldUntilMs: null,
        },
      ],
      overlay: null,
      pendingOrigin: {
        channelId: GENERAL,
        origin: "https://api.stripe.com",
        agentName: "Hermes",
      },
    });
    await expect(page.getByTestId("origin-approval-card")).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("origin-approval-card").screenshot({
      path: `${SHOTS}/05-origin-elicitation.png`,
    });
    await expect(page.getByTestId("origin-approval-card")).toHaveCount(1);
    await page.getByTestId("origin-allow-domain").click();
    const tooling = await page.evaluate((channelId) => {
      return window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("get_canvas_tooling", {
        channelId,
      });
    }, GENERAL);
    expect(JSON.stringify(tooling)).toContain("api.stripe.com");

    await seedControl(page, {
      leases: [],
      overlay: null,
      pendingOrigin: {
        channelId: GENERAL,
        origin: "https://evil.example",
        agentName: "Hermes",
      },
    });
    await expect(page.getByTestId("origin-approval-card")).toHaveCount(1);
    await page.getByTestId("origin-deny").click();
    await expect(page.getByTestId("origin-approval-card")).toHaveCount(0);
    const denied = await page.evaluate(
      () => window.__BUZZ_E2E_LAST_ORIGIN_DECISION__?.() ?? null,
    );
    expect(denied).toEqual({
      code: "origin_blocked",
      origin: "https://evil.example",
    });
  });

  test("agent boot countdown and cap conflict name the agent path", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await page.evaluate(
      (holding) => {
        window.__BUZZ_E2E_SET_GOVERNOR__?.({
          sims: [holding],
          bootedCount: 1,
          capConflict: {
            kind: "sim",
            victimChannelId: holding.channelId,
            victimName: "general",
            incomingChannelId: "other",
            incomingName: "Hermes",
            idleMs: 0,
            keepToken: "keep-e2e",
          },
        });
      },
      simHolding({
        channelId: GENERAL,
        lifecycle: "booting",
        bootElapsedMs: 4000,
      }),
    );
    await expect(page.getByTestId("sim-face-booting")).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("sim-face-booting").screenshot({
      path: `${SHOTS}/06-agent-boot-countdown.png`,
    });
  });

  test("post_evidence tagged message uses mocksig", async ({ page }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await page.waitForFunction(
      () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
    );
    await page.evaluate((id) => {
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: "Agent screenshot\n\n![](https://example/shot.png)",
        extraTags: [["crew-evidence", "before-after-visual"]],
        id,
      });
    }, MOCK_ID);
    await expect(
      page.getByTestId("evidence-card-before-after-visual"),
    ).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("evidence-card-before-after-visual").screenshot({
      path: `${SHOTS}/07-post-evidence.png`,
    });
  });
});

declare global {
  interface Window {
    __BUZZ_E2E_LAST_ORIGIN_DECISION__?: () => {
      code: string;
      origin: string;
    } | null;
  }
}
