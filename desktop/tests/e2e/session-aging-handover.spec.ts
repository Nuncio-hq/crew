/**
 * Issue #173 — session aging banner (benign) + handover note card.
 * Aging is thread-scoped; never Lost contact / Possibly stalled / Mission Inbox.
 */

import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const AGENT_PUBKEY = TEST_IDENTITIES.alice.pubkey;
const CHANNEL = "engineering";

async function waitForMockLiveSubscription(
  page: import("@playwright/test").Page,
  channelName: string,
) {
  await expect
    .poll(async () => {
      return page.evaluate((name) => {
        return (
          window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
            channelName: name,
          }) ?? false
        );
      }, channelName);
    })
    .toBe(true);
}

async function openChannel(page: import("@playwright/test").Page) {
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: AGENT_PUBKEY,
        name: "AgingAgent",
        status: "running",
        channelNames: [CHANNEL],
      },
    ],
    searchProfiles: [
      {
        pubkey: AGENT_PUBKEY,
        displayName: "AgingAgent",
        ownerPubkey: TEST_IDENTITIES.tyler.pubkey,
        isAgent: true,
      },
    ],
    mock: {
      globalAgentConfig: {
        env_vars: {},
        provider: null,
        model: null,
        preferred_runtime: null,
        handover_summarizer_model: "gpt-4o-mini",
        compaction_aging_threshold: 3,
        turn_aging_threshold: 100,
      },
    },
  });
  await page.goto("/");
  await page.getByTestId(`channel-${CHANNEL}`).click();
  await expect(page.getByTestId("chat-title")).toHaveText(CHANNEL);
  await waitForMockLiveSubscription(page, CHANNEL);
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );
}

async function emitAndOpenThread(
  page: import("@playwright/test").Page,
  content: string,
) {
  const messageId = await page.evaluate(
    ({ channelName, content, pubkey }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        content,
        pubkey,
      })?.id ?? null,
    {
      channelName: CHANNEL,
      content,
      pubkey: TEST_IDENTITIES.tyler.pubkey,
    },
  );
  expect(messageId).toBeTruthy();

  const row = page.locator(`[data-message-id="${messageId}"]`).first();
  await expect(row).toBeVisible({ timeout: 10_000 });

  const replyButton = page.locator(
    `[data-testid="reply-message-${messageId}"]`,
  );
  await expect(replyButton).toBeVisible({ timeout: 10_000 });
  await replyButton.click({ force: true });
  await expect(page.getByTestId("message-thread-panel")).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.getByTestId("message-thread-body")).toBeVisible();
  return messageId as string;
}

async function injectAging(
  page: import("@playwright/test").Page,
  input: {
    conversationId: string;
    channelId: string;
    reason: "compaction_threshold" | "turn_count_net";
    compactionCount: number;
    compactionSignal: "known" | "unknown";
    sessionTurnCount: number;
  },
) {
  await page.evaluate(
    ({ agentPubkey, ...payload }) => {
      window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
        agentPubkey,
        events: [
          {
            seq: Date.now(),
            timestamp: new Date().toISOString(),
            kind: "session_aging",
            agentIndex: 0,
            channelId: payload.channelId,
            conversationId: payload.conversationId,
            sessionId: "sess-aging",
            turnId: null,
            payload: {
              pubkey: agentPubkey,
              channelId: payload.channelId,
              conversationId: payload.conversationId,
              aging: true,
              reason: payload.reason,
              compactionCount: payload.compactionCount,
              compactionSignal: payload.compactionSignal,
              sessionTurnCount: payload.sessionTurnCount,
              compactionThreshold: 3,
              turnThreshold: 100,
            },
          },
        ],
      });
    },
    { agentPubkey: AGENT_PUBKEY, ...input },
  );
}

test.describe("session aging + handover (#173)", () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test("aging banner is benign and never failure copy", async ({ page }) => {
    await openChannel(page);
    const messageId = await emitAndOpenThread(page, "Root for aging thread");

    await injectAging(page, {
      conversationId: messageId,
      channelId: messageId,
      reason: "compaction_threshold",
      compactionCount: 3,
      compactionSignal: "known",
      sessionTurnCount: 12,
    });

    const banner = page.getByTestId("session-aging-banner");
    await expect(banner).toBeVisible({ timeout: 10_000 });
    await expect(banner).toContainText("compacted 3×");
    await expect(banner).toContainText("New session (guided handover)");
    await expect(page.getByText("Lost contact")).toHaveCount(0);
    await expect(page.getByText("Possibly stalled")).toHaveCount(0);

    await waitForAnimations(page);
    await banner.screenshot({
      path: "test-results/screenshots-session-aging/01-aging-banner.png",
    });
  });

  test("handover note card renders labeled provenance", async ({ page }) => {
    await openChannel(page);
    await page.evaluate(
      ({ channelName, pubkey }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          content:
            "## Current state\nWorking on aging UI.\n\n## Settled decisions\nThreshold is 3.",
          pubkey,
          extraTags: [["crew-handover", "gpt-4o-mini"]],
        }),
      { channelName: CHANNEL, pubkey: AGENT_PUBKEY },
    );

    const card = page.getByTestId("handover-note-card");
    await expect(card).toBeVisible();
    await expect(card).toContainText("Handover note");
    await expect(page.getByTestId("handover-note-provenance")).toContainText(
      "generated by gpt-4o-mini",
    );
    await expect(card).toContainText("Threshold is 3");

    await waitForAnimations(page);
    await card.screenshot({
      path: "test-results/screenshots-session-aging/02-handover-card.png",
    });
  });

  test("unknown-signal turn-count banner never shows a compaction number", async ({
    page,
  }) => {
    await openChannel(page);
    const messageId = await emitAndOpenThread(
      page,
      "Seed message for unknown aging",
    );

    await injectAging(page, {
      conversationId: messageId,
      channelId: messageId,
      reason: "turn_count_net",
      compactionCount: 0,
      compactionSignal: "unknown",
      sessionTurnCount: 100,
    });

    const banner = page.getByTestId("session-aging-banner");
    await expect(banner).toBeVisible({ timeout: 10_000 });
    await expect(banner).toContainText("100+ turns");
    await expect(banner).not.toContainText("compacted");
  });
});
