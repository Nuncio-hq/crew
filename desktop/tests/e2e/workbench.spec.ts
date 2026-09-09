import { openWorkspaceChannel } from "../helpers/workspaceNavigation";
import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const ENGINEERING_ID = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const ROOT_A = "1".repeat(64);
const REQUEST_ID = "4".repeat(64);
const EVIDENCE_ID = "5".repeat(64);
const OWNER = "deadbeef".repeat(8);
const HERMES = TEST_IDENTITIES.alice.pubkey;
const CODEX = TEST_IDENTITIES.bob.pubkey;

test.use({ viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

test.describe("thread session stays on the channel (#219)", () => {
  test("live job steers on the channel; inbox still picks; no workbench place", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: HERMES,
          name: "Hermes",
          status: "running",
          channelNames: ["engineering", "design"],
        },
        {
          pubkey: CODEX,
          name: "Codex",
          status: "running",
          channelNames: ["engineering", "design"],
        },
      ],
      searchProfiles: [
        {
          pubkey: HERMES,
          displayName: "Hermes",
          ownerPubkey: OWNER,
          isAgent: true,
        },
        {
          pubkey: CODEX,
          displayName: "Codex",
          ownerPubkey: OWNER,
          isAgent: true,
        },
      ],
    });

    await page.goto("/");
    await expect(page.getByTestId("open-workbench-view")).toHaveCount(0);
    await openWorkspaceChannel(page, "engineering");
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
    await seedEngineeringThread(page, now - 120);
    await injectObserver(page, ENGINEERING_ID);

    const kickoff = page.getByTestId("message-row").filter({
      hasText: "Fix reconnect freeze",
    });
    await expect(kickoff).toBeVisible();
    await kickoff.hover();
    await page.getByRole("button", { name: "Reply" }).first().click();
    const panel = page.getByTestId("message-thread-panel");
    await expect(panel).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await expect(panel.getByTestId("open-thread-workbench")).toHaveCount(0);
    await expect(page.getByTestId("workbench-screen")).toHaveCount(0);
    await expect(page.getByTestId("workbench-rail")).toHaveCount(0);
    await expect(page.getByTestId("live-job-desk")).toBeVisible();
    await expect(page.getByTestId("evidence-card-test-run")).toBeVisible();
    await expect(
      page.getByTestId(`channel-user-input-card-${REQUEST_ID}`),
    ).toBeVisible();

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
    const inspect = page
      .locator("[data-testid^='mission-inbox-inspect-']")
      .first();
    if (await inspect.isVisible()) {
      await inspect.click();
    } else {
      await page.locator("[data-testid^='mission-inbox-row-']").first().click();
    }
    await waitForAnimations(page);
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

async function seedEngineeringThread(page: Page, createdAt: number) {
  await page.evaluate(
    ({
      channelId,
      codex,
      createdAt: at,
      evidenceId,
      hermes,
      owner,
      requestId,
      rootId,
    }) => {
      const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
      const emitInput = window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__;
      if (!emit || !emitInput) throw new Error("Mock emit helpers missing.");
      emit({
        channelName: "engineering",
        content: "Fix reconnect freeze",
        createdAt: at,
        id: rootId,
        mentionPubkeys: [hermes, codex],
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
      emit({
        channelName: "engineering",
        content: "Codex finished the tests.",
        createdAt: at + 20,
        mentionPubkeys: [codex],
        parentEventId: rootId,
        pubkey: codex,
      });
      emit({
        channelName: "engineering",
        content: "Tests: 14 passed, 0 failed",
        createdAt: at + 30,
        extraTags: [["crew-evidence", "test-run"]],
        id: evidenceId,
        parentEventId: rootId,
        pubkey: hermes,
      });
      emitInput({
        channelName: "engineering",
        content: JSON.stringify({
          channel_id: channelId,
          engine: "codex",
          message: "Ship the reconnect fix?",
          questions: [
            {
              header: "Choice",
              id: "q0",
              options: [
                { description: "", label: "Yes", value: "yes" },
                { description: "", label: "No", value: "no" },
              ],
              question: "Merge?",
            },
          ],
          request_id: requestId,
          session_id: "workbench-session",
          turn_id: "workbench-turn",
        }),
        pubkey: hermes,
        requestId,
        rootEventId: rootId,
      });
    },
    {
      channelId: ENGINEERING_ID,
      codex: CODEX,
      createdAt,
      evidenceId: EVIDENCE_ID,
      hermes: HERMES,
      owner: OWNER,
      requestId: REQUEST_ID,
      rootId: ROOT_A,
    },
  );
}

async function injectObserver(page: Page, channelId: string) {
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
                sessionId: "wb-session",
                update: {
                  sessionUpdate: "tool_call",
                  status: "completed",
                  title: "bash",
                  toolCallId: "call-ls",
                  toolName: "shell",
                },
              },
            },
            seq: 2,
            sessionId: "wb-session",
            timestamp: now,
            turnId: "wb-turn",
          },
        ],
      });
    },
    { agentPubkey: HERMES, channelId },
  );
}
