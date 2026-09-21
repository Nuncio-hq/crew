import { expect, test, type Page } from "@playwright/test";

import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { openWorkspaceChannel } from "../helpers/workspaceNavigation";
import { THREAD_FOCUS_SLIVER_WIDTH_PX } from "../../src/features/channels/lib/threadFocusLayout";
import { waitForAnimations } from "../helpers/animations";
import { openSettings } from "../helpers/settings";

// #356 manual thread recap: settings save/cancel, Context Generate/Regenerate,
// stale-after-edit, failure retention, cancellation, unsupported and
// missing-selection states — all through the mock IPC boundary. No provider
// runs; fixture text stays labelled.

const CHANNEL = "general";

const SUPPORTED_CLAUDE = {
  id: "claude",
  label: "Claude Code",
  kind: "cli",
  availability: "supported" as const,
  reason: null,
  capabilityFingerprint: "fp-claude",
  profiles: [],
  models: ["certified-model"],
};

const SUPPORTED_HERMES = {
  id: "hermes",
  label: "Hermes",
  kind: "hermes",
  availability: "supported" as const,
  reason: null,
  capabilityFingerprint: "fp-hermes",
  profiles: [{ id: "research", label: "Research" }],
  models: ["hermes-certified-model"],
};

function fixtureRecap(
  generationId: string,
  text: string,
  overrides: Record<string, unknown> = {},
) {
  return {
    generationId,
    text,
    generatedAt: 1_700_000_000_000,
    runtimeId: "claude",
    requestedModel: "certified-model",
    effectiveModel: "certified-model",
    profileRef: null,
    provenance: "verified",
    sourceManifestHash: "mock-manifest-0",
    sourceEventIds: [] as string[],
    omittedMessageCount: 0,
    sourceOverflow: false,
    oldestIncludedEventId: null,
    newestIncludedEventId: null,
    ...overrides,
  };
}

type MockMessageWindow = Window & {
  __BUZZ_E2E_EMIT_MOCK_MESSAGE__?: (input: {
    channelName: string;
    content: string;
    parentEventId?: string | null;
    pubkey?: string;
    kind?: number;
    extraTags?: string[][];
    id?: string;
  }) => { id: string } | undefined;
};

async function waitForMockLiveSubscription(page: Page, channelName: string) {
  await expect
    .poll(() =>
      page.evaluate(
        (ch) =>
          (
            window as Window & {
              __BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?: (input: {
                channelName: string;
              }) => boolean;
            }
          ).__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({ channelName: ch }) ??
          false,
        channelName,
      ),
    )
    .toBe(true);
}

async function emitThreadRoot(page: Page, content: string) {
  const root = await page.evaluate(
    ({ channelName, content, pubkey }) =>
      (window as MockMessageWindow).__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        content,
        pubkey,
      }) ?? null,
    {
      channelName: CHANNEL,
      content,
      pubkey: TEST_IDENTITIES.tyler.pubkey,
    },
  );
  if (!root?.id) throw new Error("mock message emit did not return an id");
  return root.id;
}

async function emitThreadReply(
  page: Page,
  rootEventId: string,
  content: string,
) {
  await page.evaluate(
    ({ channelName, content, rootId, pubkey }) =>
      (window as MockMessageWindow).__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        content,
        pubkey,
        parentEventId: rootId,
      }),
    {
      channelName: CHANNEL,
      content,
      rootId: rootEventId,
      pubkey: TEST_IDENTITIES.tyler.pubkey,
    },
  );
}

async function emitEdit(page: Page, targetId: string, content: string) {
  await page.evaluate(
    ({ channelName, targetId, content, pubkey }) =>
      (window as MockMessageWindow).__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        content,
        pubkey,
        kind: 40003,
        extraTags: [["e", targetId]],
      }),
    {
      channelName: CHANNEL,
      targetId,
      content,
      pubkey: TEST_IDENTITIES.tyler.pubkey,
    },
  );
}

/** Pin the focus-mode thread surface so the drawer + tool pane are stable. */
async function useFocusThreadMode(page: Page) {
  await page.addInitScript(() =>
    localStorage.setItem("buzz.channels.threadViewMode", "focus"),
  );
}

async function seedChannelRoot(page: Page) {
  await openWorkspaceChannel(page, CHANNEL);
  await waitForMockLiveSubscription(page, CHANNEL);
  return emitThreadRoot(page, "Recap me: what did we decide about launch?");
}

async function openThreadContext(page: Page, rootId: string) {
  const pane = page.getByTestId("channel-tool-pane");
  if (!(await pane.isVisible().catch(() => false))) {
    const row = page.locator(`[data-message-id="${rootId}"]`).first();
    await row.hover();
    await row.getByRole("button", { name: "Reply", exact: true }).click();
    await expect(page.getByTestId("focus-thread-drawer")).toBeVisible();
    await page.getByRole("button", { name: "Open thread tools" }).click();
    await expect(pane).toBeVisible();
  }
  const contextTab = page.getByRole("tab", { name: "Context", exact: true });
  if ((await contextTab.getAttribute("aria-selected")) !== "true") {
    await contextTab.click();
  }
  await waitForAnimations(page);
}

test.describe("companyos recap (#356)", () => {
  test("fail-closed: every runtime is honestly unsupported and recap stays Off", async ({
    page,
  }) => {
    await useFocusThreadMode(page);
    await installMockBridge(page);
    await page.goto("/");

    await openSettings(page, "recap");
    await expect(page.getByTestId("settings-recap")).toBeVisible();
    const select = page.getByTestId("recap-runtime-select");
    await expect(select).toHaveValue("");
    // The inventory lists the catalogued runtimes but every option is
    // disabled — no silently-selectable fake support.
    await expect(
      select.locator("option", { hasText: "Claude Code" }),
    ).toBeDisabled();
    await expect(
      select.locator("option", { hasText: "Hermes" }),
    ).toBeDisabled();
    await expect(page.getByTestId("recap-settings-save")).toBeDisabled();

    await page.getByTestId("settings-back-to-app").click();
    const rootId = await seedChannelRoot(page);
    await openThreadContext(page, rootId);
    await expect(page.getByTestId("thread-recap")).toBeVisible();
    await expect(page.getByTestId("thread-recap-status")).toHaveText("Off");
    await expect(page.getByTestId("thread-recap-generate")).toBeDisabled();
  });

  test("Off → configure → Generate → edit → stale → failed regen keeps prior → retry", async ({
    page,
  }) => {
    const recapA = fixtureRecap("gen-a", "[fixture] Recap A: launch Friday.");
    const recapB = fixtureRecap("gen-b", "[fixture] Recap B: launch slipped.");
    await useFocusThreadMode(page);
    await installMockBridge(page, {
      recap: {
        runtimes: [SUPPORTED_CLAUDE],
        // Call 2 (the post-edit regenerate) fails; calls 1 and 3 succeed.
        generateResults: [recapA, { error: "output_limit" }, recapB],
      },
    });
    await page.goto("/");

    // Off by default: the Context panel must not offer generation yet.
    const rootId = await seedChannelRoot(page);
    await openThreadContext(page, rootId);
    await expect(page.getByTestId("thread-recap-status")).toHaveText("Off");
    await expect(page.getByTestId("thread-recap-generate")).toBeDisabled();

    // Configure manual generation through the real settings surface. Close the
    // tools pane and drawer first — their overlay intercepts sidebar clicks.
    await page
      .getByRole("button", { name: "Close Tools", exact: true })
      .click();
    await expect(page.getByTestId("channel-tool-pane")).toHaveCount(0);
    await page.getByTestId("focus-thread-drawer-scrim").click({
      position: { x: THREAD_FOCUS_SLIVER_WIDTH_PX / 2, y: 20 },
    });
    await expect(page.getByTestId("focus-thread-drawer")).toHaveCount(0);
    await openSettings(page, "recap");
    await page
      .getByTestId("recap-runtime-select")
      .selectOption({ value: "claude" });
    await expect(page.getByTestId("recap-model-input")).toHaveValue(
      "certified-model",
    );
    await expect(page.getByTestId("recap-settings-save")).toBeEnabled();
    await page.getByTestId("recap-settings-save").click();
    await expect(page.getByText("Recap settings saved.")).toBeVisible();
    await page.getByTestId("settings-back-to-app").click();

    // Generate → current with provenance and source link.
    await openWorkspaceChannel(page, CHANNEL);
    await openThreadContext(page, rootId);
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "No recap yet",
    );
    await page.getByTestId("thread-recap-generate").click();
    await expect(page.getByTestId("thread-recap-status")).toHaveText("Current");
    const result = page.getByTestId("thread-recap-result");
    await expect(result).toContainText("[fixture] Recap A: launch Friday.");
    await expect(result).toContainText("certified-model");
    await expect(
      result.getByTestId("thread-recap-source-link").first(),
    ).toBeVisible();

    // A source edit must mark the stored recap stale, never regenerate it.
    await emitEdit(
      page,
      rootId,
      "Recap me: what did we decide about launch? (edited)",
    );
    await openThreadContext(page, rootId);
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "Out of date",
    );
    await expect(result).toContainText("[fixture] Recap A: launch Friday.");

    // A failed regeneration preserves the previous result and error.
    await page.getByTestId("thread-recap-generate").click();
    await expect(page.getByTestId("thread-recap-status")).toHaveText("Failed");
    await expect(result).toContainText("[fixture] Recap A: launch Friday.");
    await expect(page.getByTestId("thread-recap-status-message")).toContainText(
      "oversized recap",
    );

    // Retry succeeds and replaces the stale artifact.
    await page.getByTestId("thread-recap-generate").click();
    await expect(page.getByTestId("thread-recap-status")).toHaveText("Current");
    await expect(result).toContainText("[fixture] Recap B: launch slipped.");

    // A new reply also invalidates the artifact (source manifest changed).
    await emitThreadReply(page, rootId, "update: we moved launch to Monday");
    await openThreadContext(page, rootId);
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "Out of date",
    );
  });

  test("save then cancel restores persisted Off; unsupported selection cannot be saved", async ({
    page,
  }) => {
    await useFocusThreadMode(page);
    await installMockBridge(page, { recap: { runtimes: [SUPPORTED_CLAUDE] } });
    await page.goto("/");
    await openSettings(page, "recap");

    await page
      .getByTestId("recap-runtime-select")
      .selectOption({ value: "claude" });
    await expect(page.getByTestId("recap-settings-save")).toBeEnabled();
    // Cancel discards the draft instead of persisting it.
    await page.getByTestId("recap-settings-cancel").click();
    await expect(page.getByTestId("recap-runtime-select")).toHaveValue("");
    await expect(page.getByTestId("recap-settings-save")).toBeDisabled();
  });

  test("missing runtime selection reports Setup needed and cannot generate", async ({
    page,
  }) => {
    // Settings reference a runtime that is no longer catalogued at all.
    await useFocusThreadMode(page);
    await installMockBridge(page, {
      recap: {
        settings: {
          mode: "manual",
          runtimeId: "ghost-cli",
          requestedModel: "certified-model",
          capabilityFingerprint: "fp-ghost",
        },
      },
    });
    await page.goto("/");
    const rootId = await seedChannelRoot(page);
    await openThreadContext(page, rootId);
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "Setup needed",
    );
    await expect(page.getByTestId("thread-recap-generate")).toBeDisabled();
  });

  test("generation cancel discards uncommitted output and stays cancelled", async ({
    page,
  }) => {
    await useFocusThreadMode(page);
    await installMockBridge(page, {
      recap: {
        runtimes: [SUPPORTED_CLAUDE],
        settings: {
          mode: "manual",
          runtimeId: "claude",
          requestedModel: "certified-model",
          capabilityFingerprint: "fp-claude",
        },
        holdGenerateUntilCancel: true,
      },
    });
    await page.goto("/");
    const rootId = await seedChannelRoot(page);
    await openThreadContext(page, rootId);
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "No recap yet",
    );

    await page.getByTestId("thread-recap-generate").click();
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "Generating",
    );
    await page.getByTestId("thread-recap-cancel").click();
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "Cancelled",
    );
    // The late rejection of the held generation must not clobber the state.
    await expect(page.getByTestId("thread-recap-status")).toHaveText(
      "Cancelled",
      { timeout: 2000 },
    );
    await expect(page.getByTestId("thread-recap-result")).toHaveCount(0);
  });

  test("hermes selection requires its certified profile before save", async ({
    page,
  }) => {
    await useFocusThreadMode(page);
    await installMockBridge(page, { recap: { runtimes: [SUPPORTED_HERMES] } });
    await page.goto("/");
    await openSettings(page, "recap");

    await page
      .getByTestId("recap-runtime-select")
      .selectOption({ value: "hermes" });
    await expect(page.getByTestId("recap-profile-select")).toBeVisible();
    await expect(page.getByTestId("recap-settings-save")).toBeDisabled();
    await page
      .getByTestId("recap-profile-select")
      .selectOption({ value: "research" });
    await expect(page.getByTestId("recap-settings-save")).toBeEnabled();
    await page.getByTestId("recap-settings-save").click();
    await expect(page.getByText("Recap settings saved.")).toBeVisible();
  });

  test("recap controls remain reachable by keyboard and in a narrow layout", async ({
    page,
  }) => {
    await useFocusThreadMode(page);
    await installMockBridge(page, {
      recap: {
        runtimes: [SUPPORTED_CLAUDE],
        settings: {
          mode: "manual",
          runtimeId: "claude",
          requestedModel: "certified-model",
          capabilityFingerprint: "fp-claude",
        },
      },
    });
    await page.goto("/");
    const rootId = await seedChannelRoot(page);
    await openThreadContext(page, rootId);

    const generate = page.getByTestId("thread-recap-generate");
    await expect(generate).toBeEnabled();
    await generate.focus();
    await expect(generate).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("thread-recap-status")).toHaveText("Current");

    // Narrow the thread drawer surface; the recap block still reads cleanly.
    await page.setViewportSize({ width: 640, height: 700 });
    await waitForAnimations(page);
    await expect(page.getByTestId("thread-recap")).toBeVisible();
    await expect(generate).toBeVisible();
    await expect(page.getByTestId("thread-recap-result")).toContainText(
      "[fixture]",
    );
  });
});
