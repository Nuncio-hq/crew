import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const SHOTS = "test-results/live-job-desk";
const ENGINEERING_ID = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const ROOT_A = "1".repeat(64);
const ROOT_QUIET = "a".repeat(64);
const REQUEST_ID = "4".repeat(64);
const OWNER = "deadbeef".repeat(8);
const HERMES = TEST_IDENTITIES.alice.pubkey;

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

test.describe("live job desk (#219)", () => {
  test("no job ⇒ no desk and no workbench-as-picker", async ({ page }) => {
    await installMockBridge(page);
    await page.goto("/");
    await expect(page.getByTestId("channel-engineering")).toBeVisible();
    await expect(page.getByTestId("open-workbench-view")).toHaveCount(0);
    await expect(page.getByTestId("workbench-empty")).toHaveCount(0);
    await expect(page.getByTestId("workbench-rail")).toHaveCount(0);

    await page.getByTestId("channel-engineering").click();
    await waitForLive(page, "engineering");
    await page.waitForFunction(
      () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
    );
    const now = Math.floor(Date.now() / 1000);
    await page.evaluate(
      ({ createdAt, id, pubkey }) => {
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "engineering",
          content: "Quiet human note. No agent job.",
          createdAt,
          id,
          pubkey,
        });
      },
      { createdAt: now - 30, id: ROOT_QUIET, pubkey: OWNER },
    );

    const kickoff = page.getByTestId("message-row").filter({
      hasText: "Quiet human note. No agent job.",
    });
    await expect(kickoff).toBeVisible();
    await kickoff.hover();
    await page.getByRole("button", { name: "Reply" }).first().click();
    const panel = page.getByTestId("message-thread-panel");
    await expect(panel).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await expect(panel.getByTestId("open-thread-workbench")).toHaveCount(0);
    await expect(page.getByTestId("live-job-desk")).toHaveCount(0);
    await expect(page.getByTestId("workbench-screen")).toHaveCount(0);
    await expect(page.getByTestId("workbench-empty")).toHaveCount(0);
    await expect(page.getByTestId("workbench-rail")).toHaveCount(0);
    await expect(page.getByText("Pick a thread from the rail")).toHaveCount(0);
    await waitForAnimations(page);
    await page.screenshot({ path: `${SHOTS}/01-no-job-no-desk.png` });

    await page.goto("/#/workbench");
    await expect(page.getByTestId("workbench-empty")).toHaveCount(0);
    await expect(page.getByTestId("workbench-rail")).toHaveCount(0);
    await expect(page.getByTestId("open-workbench-view")).toHaveCount(0);
    await expect(page.getByText("Pick a thread from the rail")).toHaveCount(0);
    await expect(page.getByTestId("home-inbox-list")).toBeVisible();
  });

  test("live job desk steers on the channel session", async ({ page }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: HERMES,
          name: "Hermes",
          status: "running",
          channelNames: ["engineering"],
        },
      ],
      searchProfiles: [
        {
          pubkey: HERMES,
          displayName: "Hermes",
          ownerPubkey: OWNER,
          isAgent: true,
        },
      ],
    });

    await page.goto("/");
    await expect(page.getByTestId("channel-engineering")).toBeVisible();
    await page.getByTestId("channel-engineering").click();
    await waitForLive(page, "engineering");
    await expect
      .poll(async () =>
        page.evaluate(
          () =>
            window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
              channelName: "engineering",
              kind: 46040,
            }) ?? false,
        ),
      )
      .toBe(true);
    await page.waitForFunction(
      () =>
        typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function" &&
        typeof window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__ === "function",
    );
    const now = Math.floor(Date.now() / 1000);
    await seedLiveJobThread(page, now - 120);
    await injectWorkingObserver(page, ENGINEERING_ID);

    const kickoff = page.getByTestId("message-row").filter({
      hasText: "Fix reconnect freeze",
    });
    await expect(kickoff).toBeVisible();
    await kickoff.hover();
    await page.getByRole("button", { name: "Reply" }).first().click();
    const panel = page.getByTestId("message-thread-panel");
    await expect(panel).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await expect(page.getByTestId("open-workbench-view")).toHaveCount(0);
    await expect(page.getByTestId("workbench-rail")).toHaveCount(0);
    await expect(page.getByTestId("workbench-empty")).toHaveCount(0);
    await expect(page.getByTestId("live-job-desk")).toBeVisible();
    await expect(page.getByTestId("live-job-desk-steer")).toBeVisible();
    await expect(page.getByTestId("live-job-desk")).not.toContainText(
      "Mention",
    );
    await waitForAnimations(page);
    await page.screenshot({ path: `${SHOTS}/02-live-job-steer.png` });

    await page.getByTestId("live-job-desk-steer").click();
    await expect(
      page.locator(
        "[data-testid='thread-composer-overlay'] [contenteditable='true']",
      ),
    ).toBeFocused();

    await page
      .getByTestId("sidebar-primary-menu")
      .getByRole("button", { name: "Inbox" })
      .click();
    await expect(page.getByTestId("home-inbox-list")).toBeVisible();
    await expect(
      page.locator("[data-testid^='mission-inbox-workbench-']"),
    ).toHaveCount(0);
  });
});

async function waitForLive(page: Page, channelName: string) {
  await expect
    .poll(async () =>
      page.evaluate(
        (name) =>
          window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
            channelName: name,
          }) ?? false,
        channelName,
      ),
    )
    .toBe(true);
}

async function seedLiveJobThread(page: Page, createdAt: number) {
  await page.evaluate(
    ({ channelId, createdAt: at, hermes, owner, requestId, rootId }) => {
      const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
      const emitInput = window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__;
      if (!emit || !emitInput) throw new Error("Mock emit helpers missing.");
      emit({
        channelName: "engineering",
        content: "Fix reconnect freeze",
        createdAt: at,
        id: rootId,
        mentionPubkeys: [hermes],
        pubkey: owner,
      });
      emit({
        channelName: "engineering",
        content: "Hermes is on the reconnect path.",
        createdAt: at + 10,
        mentionPubkeys: [hermes],
        parentEventId: rootId,
        pubkey: hermes,
      });
      emitInput({
        channelName: "engineering",
        content: JSON.stringify({
          channel_id: channelId,
          engine: "hermes",
          message: "Need a steer on the reconnect fix.",
          questions: [
            {
              header: "Choice",
              id: "q0",
              options: [
                { description: "", label: "Yes", value: "yes" },
                { description: "", label: "No", value: "no" },
              ],
              question: "Keep going?",
            },
          ],
          request_id: requestId,
          session_id: "desk-session",
          turn_id: "desk-turn",
        }),
        pubkey: hermes,
        requestId,
        rootEventId: rootId,
      });
    },
    {
      channelId: ENGINEERING_ID,
      createdAt,
      hermes: HERMES,
      owner: OWNER,
      requestId: REQUEST_ID,
      rootId: ROOT_A,
    },
  );
}

async function injectWorkingObserver(page: Page, channelId: string) {
  await page.evaluate(
    ({ agentPubkey, channelId: id }) => {
      const now = new Date().toISOString();
      window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
        agentPubkey,
        events: [
          {
            agentIndex: 0,
            channelId: id,
            kind: "acp_read",
            payload: {
              method: "session/update",
              params: {
                sessionId: "desk-session",
                update: {
                  sessionUpdate: "tool_call",
                  status: "in_progress",
                  title: "bash",
                  toolCallId: "call-ls",
                  toolName: "shell",
                },
              },
            },
            seq: 1,
            sessionId: "desk-session",
            timestamp: now,
            turnId: "desk-turn",
          },
        ],
      });
    },
    { agentPubkey: HERMES, channelId },
  );
}
