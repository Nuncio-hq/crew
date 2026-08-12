/**
 * Issue #173 — session aging banner (benign) + handover note card.
 * Aging is thread-scoped; never Lost contact / Possibly stalled / Mission Inbox.
 */

import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const AGENT_PUBKEY = TEST_IDENTITIES.alice.pubkey;
const CHANNEL = "engineering";

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
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );
}

test.describe("session aging + handover (#173)", () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test("aging banner is benign and never failure copy", async ({ page }) => {
    await openChannel(page);

    const root = await page.evaluate(
      ({ channelName, pubkey }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          content: "Root for aging thread",
          pubkey,
        }),
      { channelName: CHANNEL, pubkey: TEST_IDENTITIES.tyler.pubkey },
    );
    const rootId =
      typeof root === "object" && root && "id" in root
        ? String((root as { id: string }).id)
        : null;
    expect(rootId).toBeTruthy();

    // Open the thread so the banner slot is mounted.
    await page.getByText("Root for aging thread").click();
    await expect(page.getByTestId("message-thread-body")).toBeVisible();

    const channelId = await page.evaluate(() => {
      const el = document.querySelector("[data-channel-id]");
      return el?.getAttribute("data-channel-id");
    });

    await page.evaluate(
      ({ agentPubkey, channelId, conversationId }) => {
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey,
          events: [
            {
              seq: 1,
              timestamp: new Date().toISOString(),
              kind: "session_aging",
              agentIndex: 0,
              channelId: channelId ?? conversationId,
              conversationId,
              sessionId: "sess-aging-1",
              turnId: null,
              payload: {
                pubkey: agentPubkey,
                channelId: channelId ?? conversationId,
                conversationId,
                aging: true,
                reason: "compaction_threshold",
                compactionCount: 3,
                compactionSignal: "known",
                sessionTurnCount: 12,
                compactionThreshold: 3,
                turnThreshold: 100,
              },
            },
          ],
        });
      },
      {
        agentPubkey: AGENT_PUBKEY,
        channelId,
        conversationId: rootId,
      },
    );

    // Prefer thread-head conversation id; if banner keys on channel, still show.
    // Re-inject with channel id as conversationId for channel-pane threads.
    await page.evaluate(
      ({ agentPubkey, channelId }) => {
        if (!channelId) return;
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey,
          events: [
            {
              seq: 2,
              timestamp: new Date().toISOString(),
              kind: "session_aging",
              agentIndex: 0,
              channelId,
              conversationId: channelId,
              sessionId: "sess-aging-1",
              turnId: null,
              payload: {
                pubkey: agentPubkey,
                channelId,
                conversationId: channelId,
                aging: true,
                reason: "compaction_threshold",
                compactionCount: 3,
                compactionSignal: "known",
                sessionTurnCount: 12,
                compactionThreshold: 3,
                turnThreshold: 100,
              },
            },
          ],
        });
      },
      { agentPubkey: AGENT_PUBKEY, channelId },
    );

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
    await page.getByTestId(`channel-${CHANNEL}`).click();

    const channelId = await page.evaluate(async () => {
      // Navigate stays on channel; use mock channel id from DOM if present.
      const link = document.querySelector(`[data-testid="channel-engineering"]`);
      return link?.getAttribute("data-channel-id") ?? "engineering";
    });

    await page.evaluate(
      ({ agentPubkey, channelId }) => {
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey,
          events: [
            {
              seq: 3,
              timestamp: new Date().toISOString(),
              kind: "session_aging",
              agentIndex: 0,
              channelId,
              conversationId: channelId,
              sessionId: "sess-unknown",
              turnId: null,
              payload: {
                pubkey: agentPubkey,
                channelId,
                conversationId: channelId,
                aging: true,
                reason: "turn_count_net",
                compactionCount: 0,
                compactionSignal: "unknown",
                sessionTurnCount: 100,
                compactionThreshold: 3,
                turnThreshold: 100,
              },
            },
          ],
        });
      },
      { agentPubkey: AGENT_PUBKEY, channelId },
    );

    // Channel pane may not show thread banner without an open thread — open any message thread.
    await page.evaluate(
      ({ channelName }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName,
          content: "Seed message for unknown aging",
        }),
      { channelName: CHANNEL },
    );
    await page.getByText("Seed message for unknown aging").click();
    await expect(page.getByTestId("message-thread-body")).toBeVisible();

    // Re-key aging to the thread head id after open.
    const threadId = await page.evaluate(() => {
      const head = document.querySelector(
        "[data-testid='message-thread-head'] [data-message-id]",
      );
      return head?.getAttribute("data-message-id") ?? null;
    });

    await page.evaluate(
      ({ agentPubkey, channelId, conversationId }) => {
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey,
          events: [
            {
              seq: 4,
              timestamp: new Date().toISOString(),
              kind: "session_aging",
              agentIndex: 0,
              channelId,
              conversationId: conversationId ?? channelId,
              sessionId: "sess-unknown",
              turnId: null,
              payload: {
                pubkey: agentPubkey,
                channelId,
                conversationId: conversationId ?? channelId,
                aging: true,
                reason: "turn_count_net",
                compactionCount: 0,
                compactionSignal: "unknown",
                sessionTurnCount: 100,
                compactionThreshold: 3,
                turnThreshold: 100,
              },
            },
          ],
        });
      },
      {
        agentPubkey: AGENT_PUBKEY,
        channelId,
        conversationId: threadId,
      },
    );

    const banner = page.getByTestId("session-aging-banner");
    await expect(banner).toBeVisible({ timeout: 10_000 });
    await expect(banner).toContainText("100+ turns");
    await expect(banner).not.toContainText("compacted");
  });
});
