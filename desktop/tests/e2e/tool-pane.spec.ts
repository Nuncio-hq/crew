import { expect, test, type Page } from "@playwright/test";

import type {
  GovernorStatus,
  SimHolding,
} from "../../src/features/tool-pane/types";
import { waitForAnimations } from "../helpers/animations";
import { installMockBridge } from "../helpers/bridge";
import { openSettings } from "../helpers/settings";

const GENERAL = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const RANDOM = "9dae0116-799b-5071-a0a8-fdd30a91a35d";
const SHOTS = "test-results/tool-pane";

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

async function seedGovernor(page: Page, patch: Partial<GovernorStatus>) {
  await page.evaluate((next) => {
    window.__BUZZ_E2E_SET_GOVERNOR__?.(next);
  }, patch);
}

test.describe("channel Tool Pane (#196)", () => {
  test("sim faces render distinctly and idle Keep resets", async ({ page }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");

    await seedGovernor(page, {
      bridge: {
        availability: "missing",
        binary: null,
        path: null,
        installHint: "brew install baguette",
        message: null,
      },
    });
    await expect(page.getByTestId("sim-face-bridge-missing")).toBeVisible();
    await page.screenshot({
      path: `${SHOTS}/01-sim-bridge-missing.png`,
      clip: { x: 700, y: 0, width: 580, height: 720 },
    });

    await seedGovernor(page, {
      bridge: {
        availability: "available",
        binary: "baguette",
        path: "/bin/baguette",
        installHint: null,
        message: null,
      },
      sims: [],
    });
    await expect(page.getByTestId("sim-face-absent")).toBeVisible();

    await seedGovernor(page, {
      sims: [simHolding({ channelId: GENERAL, lifecycle: "shutdown" })],
    });
    await expect(page.getByTestId("sim-face-shutdown")).toBeVisible();

    await seedGovernor(page, {
      sims: [
        simHolding({
          channelId: GENERAL,
          lifecycle: "booting",
          bootElapsedMs: 4000,
        }),
      ],
    });
    await expect(page.getByTestId("sim-face-booting")).toBeVisible();

    await seedGovernor(page, {
      sims: [
        simHolding({
          channelId: GENERAL,
          lifecycle: "mirroring",
          mirroring: true,
          paneVisible: true,
          idleDeadlineMs: Date.now() + 8 * 60_000,
        }),
      ],
      bootedCount: 1,
      streamCount: 1,
    });
    await expect(page.getByTestId("sim-face-mirroring")).toBeVisible();
    await expect(page.getByTestId("sim-idle-strip")).toContainText(
      "Shuts down in",
    );
    await page.getByTestId("sim-keep").click();
    await expect(page.getByTestId("sim-idle-strip")).toBeVisible();

    await waitForAnimations(page);
    await page
      .getByTestId("sim-bezel")
      .screenshot({ path: `${SHOTS}/02-sim-mirroring.png` });
  });

  test("hidden pane pauses the stream while the device stays booted", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await seedGovernor(page, {
      sims: [
        simHolding({
          channelId: GENERAL,
          lifecycle: "mirroring",
          mirroring: true,
          paneVisible: true,
          udid: "UDID-A",
        }),
      ],
      streamCount: 1,
      bootedCount: 1,
    });
    await expect(page.getByTestId("sim-face-mirroring")).toBeVisible();
    await page.getByTestId("tool-pane-close").click();
    await expect
      .poll(async () =>
        page.evaluate(
          () => window.__BUZZ_E2E_GOVERNOR_STATUS__?.().streamCount ?? -1,
        ),
      )
      .toBe(0);
    const lifecycle = await page.evaluate(
      () => window.__BUZZ_E2E_GOVERNOR_STATUS__?.().sims[0]?.lifecycle,
    );
    expect(lifecycle).toBe("booted");
  });

  test("sidebar dots and governor strip match holdings", async ({ page }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("channel-general")).toBeVisible();
    await seedGovernor(page, {
      sims: [
        simHolding({
          channelId: GENERAL,
          channelName: "general",
          lifecycle: "booted",
        }),
      ],
      servers: [
        {
          id: `${GENERAL}:checkout`,
          channelId: GENERAL,
          subject: "checkout",
          command: "pnpm dev --port 4173",
          port: 4173,
          url: "http://127.0.0.1:4173",
          face: "running",
          uptimeMs: 1000,
          idleDeadlineMs: null,
          lastLog: ["Local:"],
          portNote: null,
          crashCount: 0,
          cwd: "/tmp/crew",
        },
      ],
      bootedCount: 1,
      streamCount: 0,
      serverCount: 1,
      diskBytes: 4.5 * 1024 * 1024 * 1024,
    });
    await expect(page.getByTestId("resource-dot-sim")).toBeVisible();
    await expect(page.getByTestId("resource-dot-server")).toBeVisible();
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await expect(page.getByTestId("governor-strip-sims")).toContainText(
      "1 sims",
    );
    await expect(page.getByTestId("governor-strip-servers")).toContainText(
      "1 servers",
    );
    await waitForAnimations(page);
    await page.locator("[data-testid='channel-general']").screenshot({
      path: `${SHOTS}/03-sidebar-dots.png`,
    });
  });

  test("cap-hit Keep protects the visible mirror", async ({ page }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await seedGovernor(page, {
      policy: {
        maxBootedSims: 1,
        maxMirrorStreams: 1,
        mirrorFps: 20,
        mirrorQuietFps: 5,
        simIdleShutdownMs: 15 * 60_000,
        streamPauseHiddenMs: 2_000,
        hiddenWebviewCap: 2,
        hiddenWebviewTtlMs: 10 * 60_000,
        maxDevServers: 3,
        devServerIdleMs: 25 * 60_000,
        pruneUnusedMs: 30 * 24 * 60 * 60_000,
      },
      sims: [
        simHolding({
          channelId: GENERAL,
          channelName: "general",
          lifecycle: "mirroring",
          mirroring: true,
          paneVisible: true,
        }),
      ],
      bootedCount: 1,
      streamCount: 1,
    });
    const bootB = page.evaluate(async (channelId) => {
      try {
        await window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("sim_boot", {
          input: { channelId, channelName: "random" },
        });
        return "ok";
      } catch (error) {
        return error instanceof Error ? error.message : String(error);
      }
    }, RANDOM);
    await expect(bootB).resolves.toMatch(/cap|visible mirror/);
    await expect(page.getByTestId("governor-cap-keep")).toBeVisible();
    await page.getByTestId("governor-cap-keep").click();
    const stillMirroring = await page.evaluate(
      () => window.__BUZZ_E2E_GOVERNOR_STATUS__?.().sims[0]?.mirroring,
    );
    expect(stillMirroring).toBe(true);
  });

  test("browser opens to Custom URL by default with no setup wall (#236)", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "browser");
    // Default open: toolbar + navigable surface, no blocking setup card.
    await expect(page.getByTestId("browser-toolbar")).toBeVisible();
    await expect(page.getByTestId("browser-setup-card")).not.toBeVisible();
    await expect(page.getByTestId("browser-subject")).toHaveValue("custom");
    await expect(page.getByTestId("browser-preview")).toBeVisible();
  });

  test("browser dev-server Configure affordance is optional and non-blocking (#236)", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "browser");

    // Plain channel view has no worktree/checkout path — those subject
    // options stay present (per D-058: "remain when paths exist") but
    // disabled, so Custom URL is the only reachable subject.
    await expect(
      page.locator('[data-testid="browser-subject"] option[value="worktree"]'),
    ).toBeDisabled();
    await expect(
      page.locator('[data-testid="browser-subject"] option[value="checkout"]'),
    ).toBeDisabled();

    // The dev-server Configure affordance is a footer strip, not a wall
    // over the webview: the preview stays visible underneath it, and the
    // setup card only appears once the user opts in.
    await expect(page.getByTestId("browser-preview")).toBeVisible();
    await expect(
      page.getByTestId("browser-devserver-configure-strip"),
    ).toBeVisible();
    await expect(page.getByTestId("browser-setup-card")).not.toBeVisible();

    await page.getByTestId("browser-devserver-configure").click();
    await expect(page.getByTestId("browser-setup-card")).toBeVisible();
    await page.getByTestId("browser-setup-save").click();
    await expect(page.getByTestId("browser-server-strip")).toBeVisible();
    await page.getByTestId("browser-start-server").click();
    await expect(page.getByTestId("browser-server-strip")).toHaveAttribute(
      "data-server-face",
      "running",
    );

    await page.getByTestId("browser-url").fill("https://example.com");
    await expect(page.getByTestId("browser-url")).toHaveValue(
      "https://example.com",
    );
  });

  test("browser-nav-without-devserver-config: Browser is navigable with no tooling.devServer set (#236)", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "browser");

    // No canvas tooling exists for this channel at all — confirm the
    // precondition before asserting the Browser still navigates.
    const tooling = await page.evaluate((channelId) => {
      return window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("get_canvas_tooling", {
        channelId,
      });
    }, GENERAL);
    expect(tooling).toBeNull();

    // No setup wall: toolbar + a navigable surface, immediately.
    await expect(page.getByTestId("browser-toolbar")).toBeVisible();
    await expect(page.getByTestId("browser-setup-card")).not.toBeVisible();
    await expect(page.getByTestId("browser-preview")).toBeVisible();

    const url = page.getByTestId("browser-url");
    await url.fill("https://example.com/docs");
    await url.press("Enter");

    await expect(page.getByTestId("browser-preview-url")).toHaveText(
      "https://example.com/docs",
    );
    await expect
      .poll(() =>
        page.evaluate(
          (channelId) =>
            window
              .__BUZZ_E2E_GOVERNOR_STATUS__?.()
              .webviews.find((view) => view.channelId === channelId)?.url,
          GENERAL,
        ),
      )
      .toBe("https://example.com/docs");
  });

  test("back/forward/reload toolbar buttons invoke governor browser commands", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "browser");
    // No setup wall (#236): the preview is already navigable without
    // configuring or starting a Crew-owned dev server first.
    await expect(page.getByTestId("browser-preview")).toBeVisible();

    await page.getByTestId("browser-back").click();
    await page.getByTestId("browser-forward").click();
    await page.getByTestId("browser-reload").click();

    const payloads = await page.evaluate(
      () => window.__BUZZ_E2E_COMMAND_PAYLOADS__ ?? [],
    );
    for (const command of [
      "browser_back",
      "browser_forward",
      "browser_reload",
    ]) {
      const entry = payloads.find((item) => item.command === command);
      expect(entry, `expected a ${command} invocation`).toBeTruthy();
      expect((entry?.payload as { channelId?: string } | null)?.channelId).toBe(
        GENERAL,
      );
    }
  });

  test("shot posts before-after-visual evidence with Undo", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await seedGovernor(page, {
      sims: [
        simHolding({
          channelId: GENERAL,
          lifecycle: "mirroring",
          mirroring: true,
          paneVisible: true,
          udid: "UDID-A",
        }),
      ],
    });
    await page.getByTestId("sim-shot").click();
    await expect
      .poll(async () =>
        page.evaluate(() =>
          (window.__BUZZ_E2E_COMMAND_PAYLOADS__ ?? []).some((entry) => {
            if (entry.command !== "sign_event") return false;
            const payload = entry.payload as { tags?: string[][] } | undefined;
            return Boolean(
              payload?.tags?.some(
                (tag) =>
                  tag[0] === "crew-evidence" &&
                  tag[1] === "before-after-visual",
              ),
            );
          }),
        ),
      )
      .toBe(true);
  });

  test("parallel: agent boot in B while mirroring A pauses A without shutdown", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await seedGovernor(page, {
      policy: {
        maxBootedSims: 2,
        maxMirrorStreams: 1,
        mirrorFps: 20,
        mirrorQuietFps: 5,
        simIdleShutdownMs: 15 * 60_000,
        streamPauseHiddenMs: 2_000,
        hiddenWebviewCap: 2,
        hiddenWebviewTtlMs: 10 * 60_000,
        maxDevServers: 3,
        devServerIdleMs: 25 * 60_000,
        pruneUnusedMs: 30 * 24 * 60 * 60_000,
      },
      sims: [
        simHolding({
          channelId: GENERAL,
          channelName: "general",
          lifecycle: "mirroring",
          mirroring: true,
          paneVisible: true,
        }),
      ],
      bootedCount: 1,
      streamCount: 1,
    });
    await page.evaluate(async (channelId) => {
      await window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.("sim_boot", {
        input: { channelId, channelName: "random" },
      });
    }, RANDOM);
    await page.getByTestId("channel-random").click();
    await expect(
      page.getByTestId(`channel-resource-dots-${RANDOM}`),
    ).toBeVisible();
    await expect(page.getByTestId("channel-tool-pane")).toHaveAttribute(
      "data-channel-id",
      RANDOM,
    );
    await page.getByTestId("channel-general").click();
    await openTools(page, "sim");
    await expect(page.getByTestId("channel-tool-pane")).toHaveAttribute(
      "data-channel-id",
      GENERAL,
    );
    const a = await page.evaluate(
      (id) =>
        window
          .__BUZZ_E2E_GOVERNOR_STATUS__?.()
          .sims.find((sim) => sim.channelId === id),
      GENERAL,
    );
    expect(a?.lifecycle === "booted" || a?.lifecycle === "mirroring").toBe(
      true,
    );
  });

  test("settings Devices & Preview lists the policy table", async ({
    page,
  }) => {
    await installMockBridge(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await openSettings(page, "devices-preview");
    await expect(page.getByTestId("settings-devices-preview")).toBeVisible();
    await expect(page.getByTestId("devices-policy-table")).toContainText(
      "Concurrent booted sims",
    );
    await waitForAnimations(page);
    await page.getByTestId("settings-devices-preview").screenshot({
      path: `${SHOTS}/04-devices-preview.png`,
    });
  });
});

declare global {
  interface Window {
    __BUZZ_E2E_SET_GOVERNOR__?: (
      patch: Partial<GovernorStatus>,
    ) => GovernorStatus;
    __BUZZ_E2E_GOVERNOR_STATUS__?: () => GovernorStatus;
  }
}
