import { createHash } from "node:crypto";
import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { waitForAnimations } from "../helpers/animations";

const AGENT_PUBKEY =
  "554cef57437abac34522ac2c9f0490d685b72c80478cf9f7ed6f9570ee8624ea";
const CHANNEL_ID = "94a444a4-c0a3-5966-ab05-530c6ddc2301";
const CONVERSATION_ID = "afab2e62-a520-f16b-e63d-b291c2f679c9";
const USER_INPUT_ROOT_EVENT_ID = "c".repeat(64);
const USER_INPUT_REQUEST_ID = "d".repeat(64);

function seedAgent() {
  return {
    managedAgents: [
      {
        pubkey: AGENT_PUBKEY,
        name: "Charlie",
        status: "running" as const,
        channelNames: ["agents"],
      },
    ],
  };
}

async function waitForTurnSeed(page: import("@playwright/test").Page) {
  await page.waitForFunction(
    () =>
      typeof (window as Window & { __BUZZ_E2E_SEED_ACTIVE_TURNS__?: unknown })
        .__BUZZ_E2E_SEED_ACTIVE_TURNS__ === "function",
    null,
    { timeout: 10_000 },
  );
}

async function seedTurn(
  page: import("@playwright/test").Page,
  kind: "turn_started" | "turn_completed",
  conversationId = CONVERSATION_ID,
) {
  await page.evaluate(
    ({ agentPubkey, channelId, conversationId, kind }) => {
      const win = window as Window & {
        __BUZZ_E2E_SEED_ACTIVE_TURNS__?: (input: {
          agentPubkey: string;
          channelId: string;
          turnId: string;
          conversationId: string;
          kind: "turn_started" | "turn_completed";
        }) => void;
      };
      win.__BUZZ_E2E_SEED_ACTIVE_TURNS__?.({
        agentPubkey,
        channelId,
        turnId: "presence-turn-1",
        conversationId,
        kind,
      });
    },
    {
      agentPubkey: AGENT_PUBKEY,
      channelId: CHANNEL_ID,
      conversationId,
      kind,
    },
  );
}

test.describe("channel header agent presence", () => {
  test("renders working and done dots with distinct visual states", async ({
    page,
  }) => {
    await installMockBridge(page, seedAgent());
    await page.goto("/");
    await page.getByTestId("channel-agents").click();
    await expect(page.getByTestId("chat-title")).toHaveText("agents");
    await waitForTurnSeed(page);

    await seedTurn(page, "turn_started");
    const presence = page.getByTestId("channel-agent-presence");
    await expect(presence).toBeVisible();
    await expect(
      presence.getByTestId("channel-agent-presence-dot-working"),
    ).toBeVisible();
    await waitForAnimations(page);
    const workingScreenshot = await presence.screenshot();

    await seedTurn(page, "turn_completed");
    await expect(
      presence.getByTestId("channel-agent-presence-dot-done-recent"),
    ).toBeVisible();
    await waitForAnimations(page);
    const doneScreenshot = await presence.screenshot();

    const hash = (value: Buffer) =>
      createHash("sha256").update(value).digest("hex");
    expect(hash(workingScreenshot)).not.toBe(hash(doneScreenshot));
  });

  test("shows needs-you for a 46040 request and opens its real thread", async ({
    page,
  }) => {
    await installMockBridge(page, seedAgent());
    await page.goto("/");
    await page.getByTestId("channel-agents").click();
    await expect(page.getByTestId("chat-title")).toHaveText("agents");
    await waitForTurnSeed(page);
    await page.waitForFunction(() =>
      window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
        channelName: "agents",
        kind: 46040,
      }),
    );
    await page.evaluate(
      ({ agentPubkey, rootEventId }) => {
        const win = window as Window & {
          __BUZZ_E2E_EMIT_MOCK_MESSAGE__?: (input: {
            channelName: string;
            content: string;
            id: string;
            kind: number;
            mentionPubkeys?: string[];
            pubkey?: string;
          }) => unknown;
        };
        win.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "agents",
          content: "Mock channel question parent",
          id: rootEventId,
          kind: 9,
          mentionPubkeys: [agentPubkey],
        });
      },
      {
        agentPubkey: AGENT_PUBKEY,
        rootEventId: USER_INPUT_ROOT_EVENT_ID,
      },
    );
    await page.evaluate(
      ({ agentPubkey, channelId, conversationId, requestId, rootEventId }) => {
        const win = window as Window & {
          __BUZZ_E2E_EMIT_MOCK_USER_INPUT__?: (input: {
            channelName: string;
            requestId?: string;
            rootEventId: string;
            parentEventId?: string;
            content: string;
            pubkey?: string;
          }) => unknown;
        };
        win.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__?.({
          channelName: "agents",
          requestId,
          rootEventId,
          parentEventId: rootEventId,
          content: JSON.stringify({
            request_id: "presence-question",
            session_id: "presence-session",
            turn_id: "presence-turn-1",
            conversation_id: conversationId,
            channel_id: channelId,
            engine: "claude",
            message: "Choose a deployment target",
            questions: [
              {
                id: "target",
                header: "Target",
                question: "Where should this run?",
                options: [],
              },
            ],
          }),
          pubkey: agentPubkey,
        });
      },
      {
        agentPubkey: AGENT_PUBKEY,
        channelId: CHANNEL_ID,
        conversationId: CONVERSATION_ID,
        requestId: USER_INPUT_REQUEST_ID,
        rootEventId: USER_INPUT_ROOT_EVENT_ID,
      },
    );
    await expect(
      page.getByText("Choose a deployment target", { exact: true }),
    ).toBeVisible();
    await seedTurn(page, "turn_started");

    const presence = page.getByTestId("channel-agent-presence");
    await expect(presence).toBeVisible();
    await expect(
      presence.getByTestId("channel-agent-presence-dot-needs-you"),
    ).toBeVisible();
    await waitForAnimations(page);
    const needsYouScreenshot = await presence.screenshot();
    expect(needsYouScreenshot.length).toBeGreaterThan(0);

    await page.getByTestId(`channel-agent-presence-${AGENT_PUBKEY}`).click();
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
  });
});
