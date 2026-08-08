import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const SHOTS = "test-results/mission-inbox";
const CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const ROOT_ID = "1".repeat(64);
const REQUEST_ID = ROOT_ID;
const CONVERSATION_ID = "2096b1ca-3834-7197-6a2a-bc5b580e07e6";

const MOCK_PUBKEY = "deadbeef".repeat(8);
const RECEIPT_ROOT_ID = "2".repeat(64);
const RECEIPT_ID = "3".repeat(64);

test.describe("mission inbox", () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test("ingests a live 46040 request, falls back to its channel, and resolves it", async ({
    page,
  }) => {
    await installMockBridge(page, {
      searchProfiles: [
        {
          pubkey: TEST_IDENTITIES.alice.pubkey,
          displayName: "Alice Agent",
          ownerPubkey: MOCK_PUBKEY,
          isAgent: true,
        },
      ],
    });
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
      {
        channelId: CHANNEL_ID,
        id: REQUEST_ID,
        pubkey: TEST_IDENTITIES.alice.pubkey,
      },
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
      ({ requestAgentPubkey, requestEventId }) =>
        window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT_ANSWER__?.({
          channelName: "general",
          requestEventId,
          requestAgentPubkey,
        }),
      {
        requestAgentPubkey: TEST_IDENTITIES.alice.pubkey,
        requestEventId: REQUEST_ID,
      },
    );
    await expect(
      page.getByTestId(`mission-inbox-row-${CONVERSATION_ID}`),
    ).toHaveCount(0);
  });

  test("surfaces heartbeat-only work as possibly stalled with recovery actions", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: MOCK_PUBKEY,
          name: "Recovery Agent",
          status: "running",
          channelNames: ["general"],
        },
      ],
    });
    await page.clock.install({ time: new Date("2026-08-08T12:00:00Z") });
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-inbox-list")).toBeVisible();

    await page.evaluate(
      ({ agentPubkey, channelId }) => {
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey,
          events: [
            {
              seq: 1,
              timestamp: new Date().toISOString(),
              kind: "turn_started",
              agentIndex: 0,
              channelId,
              conversationId: "stalled-conversation",
              sessionId: "stalled-session",
              turnId: "stalled-turn",
              payload: null,
            },
          ],
        });
      },
      { agentPubkey: MOCK_PUBKEY, channelId: CHANNEL_ID },
    );
    await page.clock.fastForward(95_000);
    await page.evaluate(
      ({ agentPubkey, channelId }) => {
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey,
          events: [
            {
              seq: 2,
              timestamp: new Date().toISOString(),
              kind: "turn_liveness",
              agentIndex: 0,
              channelId,
              conversationId: "stalled-conversation",
              sessionId: "stalled-session",
              turnId: "stalled-turn",
              payload: null,
            },
          ],
        });
      },
      { agentPubkey: MOCK_PUBKEY, channelId: CHANNEL_ID },
    );

    const row = page.locator('[data-state="possiblyStalled"]');
    await expect(row).toContainText("Possibly stalled");
    await expect(
      page.getByTestId("mission-inbox-inspect-stalled-conversation"),
    ).toBeVisible();
    const wait = page.getByTestId("mission-inbox-wait-stalled-conversation");
    await expect(wait).toHaveText("Wait 10m");
    await waitForAnimations(page);
    await row.screenshot({ path: `${SHOTS}/03-possibly-stalled.png` });

    await wait.click();
    await expect(row).toHaveCount(0);
  });

  test("surfaces unavailable observer telemetry without calling it stalled", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: MOCK_PUBKEY,
          name: "Recovery Agent",
          status: "running",
          channelNames: ["general"],
        },
      ],
    });
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("mission-inbox-sections")).toBeVisible();

    await page.evaluate(
      ({ agentPubkey, channelId }) => {
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey,
          events: [
            {
              seq: 1,
              timestamp: new Date().toISOString(),
              kind: "turn_started",
              channelId,
              turnId: "telemetry-turn",
              conversationId: "telemetry-conversation",
              payload: null,
            },
          ],
        });
        window.__BUZZ_E2E_SET_OBSERVER_CONNECTION_STATE__?.("error");
      },
      { agentPubkey: MOCK_PUBKEY, channelId: CHANNEL_ID },
    );

    const telemetry = page.locator('[data-state="telemetryUnavailable"]');
    await expect(telemetry).toContainText("Telemetry unavailable");
    await expect(telemetry).not.toContainText("Possibly stalled");
    await waitForAnimations(page);
    await telemetry.screenshot({
      path: `${SHOTS}/05-telemetry-unavailable.png`,
    });
  });

  test("uses a durable receipt for review and keeps request changes in-thread", async ({
    page,
  }) => {
    await installMockBridge(page, {
      searchProfiles: [
        {
          pubkey: TEST_IDENTITIES.alice.pubkey,
          displayName: "Alice Agent",
          ownerPubkey: MOCK_PUBKEY,
          isAgent: true,
        },
      ],
    });
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-inbox-list")).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
              channelName: "general",
              kind: 46043,
            }) ?? false,
        ),
      )
      .toBe(true);

    await page.evaluate(
      ({ agentPubkey, channelId, receiptId, rootId }) => {
        const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
        const push = window.__BUZZ_E2E_PUSH_MOCK_FEED_ITEM__;
        if (!emit || !push)
          throw new Error("Mock feed helpers are unavailable.");
        emit({
          channelName: "general",
          content: "Reviewable mission root",
          id: rootId,
          pubkey: agentPubkey,
        });
        const receipt = emit({
          channelName: "general",
          content: JSON.stringify({
            summary: "Recovery slice completed",
            verify: "pnpm check passed",
            lights: [{ label: "Desktop", status: "green" }],
            engineering: {
              pr_ref: null,
              branch: "feat/agent-attention-recovery",
              files_changed: ["agentAttention.ts"],
              ci: [{ label: "local", status: "passed" }],
            },
          }),
          id: receiptId,
          kind: 46043,
          parentEventId: rootId,
          pubkey: agentPubkey,
        });
        push({
          id: receipt.id,
          kind: receipt.kind,
          pubkey: receipt.pubkey,
          content: receipt.content,
          created_at: receipt.created_at,
          channel_id: channelId,
          channel_name: "general",
          tags: receipt.tags,
          category: "activity",
        });
      },
      {
        agentPubkey: TEST_IDENTITIES.alice.pubkey,
        channelId: CHANNEL_ID,
        receiptId: RECEIPT_ID,
        rootId: RECEIPT_ROOT_ID,
      },
    );

    const ready = page.getByTestId("mission-inbox-section-readyToReview");
    await expect(ready).toContainText("Recovery slice completed");
    await ready.getByText("Recovery slice completed").click();
    await expect(page).toHaveURL(new RegExp(`/channels/${CHANNEL_ID}`));
    const card = page.getByTestId("agent-receipt-card");
    await expect(card).toBeVisible();
    await expect(card.getByTestId("agent-receipt-reviewed")).toBeVisible();
    await expect(
      card.getByTestId("agent-receipt-request-changes"),
    ).toBeVisible();
    await waitForAnimations(page);
    await card.screenshot({ path: `${SHOTS}/04-receipt-actions.png` });

    await card.getByTestId("agent-receipt-request-changes").click();
    await expect(page.getByTestId("reply-target")).toBeVisible();
    await card.getByTestId("agent-receipt-reviewed").click();
    await page.getByRole("button", { name: "Inbox" }).click();
    await expect(page.getByTestId("mission-inbox-sections")).toBeVisible();
    await expect(ready).not.toContainText("Recovery slice completed");
  });
});
