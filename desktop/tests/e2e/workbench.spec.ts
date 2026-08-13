import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const SHOTS = "test-results/workbench";
const ENGINEERING_ID = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const ROOT_A = "1".repeat(64);
const REQUEST_ID = "4".repeat(64);
const EVIDENCE_ID = "5".repeat(64);
const CATCH_UP_ID = "6".repeat(64);
const OWNER = "deadbeef".repeat(8);
const HERMES = TEST_IDENTITIES.alice.pubkey;
const CODEX = TEST_IDENTITIES.bob.pubkey;

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

test.describe("thread workbench (#186)", () => {
  test("rail lenses, office filter, target chip, catch-up, and shared cards", async ({
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
      threadGitHubByBranch: {
        "buzz/fix-reconnect": {
          availability: "available",
          pullRequest: {
            additions: 40,
            baseRefName: "main",
            changedFiles: 3,
            checks: [
              {
                name: "Desktop Fast",
                state: "FAILURE",
                url: null,
                workflow: "CI",
              },
            ],
            closingIssuesReferences: [],
            comments: [],
            deletions: 8,
            headRefName: "buzz/fix-reconnect",
            isDraft: false,
            mergeStateStatus: "CLEAN",
            number: 186,
            reviewDecision: "REVIEW_REQUIRED",
            state: "OPEN",
            title: "Fix reconnect freeze",
            url: "https://github.com/Nuncio-hq/crew/pull/186",
          },
        },
      },
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
    await expect(panel.getByTestId("evidence-card-test-run")).toBeVisible();
    await expect(panel.getByTestId("open-thread-workbench")).toBeVisible();
    await waitForAnimations(page);
    await panel.getByTestId("open-thread-workbench").click();

    await expect(page.getByTestId("workbench-screen")).toBeVisible();
    await expect(page.getByTestId("workbench-thread")).toBeVisible();
    await expect(
      page.getByTestId(`workbench-rail-row-${ROOT_A}`),
    ).toBeVisible();
    await expect(page.getByTestId("workbench-target-chip")).toContainText(
      "Hermes",
    );
    await expect(page.getByTestId("workbench-stop")).toContainText(
      "Stop Hermes",
    );
    await expect(page.getByTestId("workbench-agent-bar")).toContainText(
      "Hermes",
    );
    await expect(page.getByTestId("workbench-agent-bar")).toContainText(
      "Codex",
    );
    await expect(page.getByTestId("evidence-card-test-run")).toBeVisible();
    await expect(
      page.getByTestId(`channel-user-input-card-${REQUEST_ID}`),
    ).toBeVisible();
    await expect(page.getByTestId("evidence-cross-check-badge")).toBeVisible({
      timeout: 15_000,
    });

    await waitForAnimations(page);
    await page.getByTestId("workbench-rail").screenshot({
      path: `${SHOTS}/01-rail-by-thread.png`,
    });
    await page.getByTestId("workbench-screen").screenshot({
      path: `${SHOTS}/03-workbench-full.png`,
    });
    await page
      .getByTestId(`channel-user-input-card-${REQUEST_ID}`)
      .screenshot({ path: `${SHOTS}/05-question-card.png` });
    await page.getByTestId("evidence-card-test-run").screenshot({
      path: `${SHOTS}/06-evidence-badge.png`,
    });

    const workbenchUrl = page.url();
    await page.getByTestId("workbench-lens-agent").click();
    await expect(page.getByTestId("workbench-lens-agent")).toBeVisible();
    await expect(
      page.getByTestId(`workbench-rail-row-${ROOT_A}`).first(),
    ).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("workbench-rail").screenshot({
      path: `${SHOTS}/02-rail-by-agent.png`,
    });
    await page.getByTestId(`workbench-rail-row-${ROOT_A}`).first().click();
    await expect(page.getByTestId("workbench-thread")).toBeVisible();
    expect(page.url().split("?")[0]).toBe(workbenchUrl.split("?")[0]);

    await expect(page.getByTestId("workbench-target-chip")).toContainText(
      "Hermes",
    );
    await page.getByTestId("workbench-target-chip").focus();
    await page.keyboard.press("Tab");
    await expect(page.getByTestId("workbench-target-chip")).toContainText(
      "Codex",
    );
    await expect(page.getByTestId("workbench-stop")).toContainText(
      "Stop Codex",
    );

    await expect(
      page.locator("[data-testid^='workbench-tool-row-']"),
    ).toBeVisible();
    await expect(page.getByTestId("workbench-role-check")).toBeVisible();

    await page.getByTestId("workbench-office-toggle").click();
    await expect(page.getByTestId("workbench-office-bar")).toBeVisible();
    await expect(
      page.getByTestId("workbench-office-composer-hidden"),
    ).toBeVisible();
    await expect(page.getByTestId("workbench-role-check")).toHaveCount(0);
    await expect(
      page.locator("[data-testid^='workbench-tool-row-']"),
    ).toHaveCount(0);
    await expect(page.getByTestId("evidence-card-test-run")).toBeVisible();
    await expect(
      page.getByTestId(`channel-user-input-card-${REQUEST_ID}`),
    ).toBeVisible();
    await waitForAnimations(page);
    await page.getByTestId("workbench-screen").screenshot({
      path: `${SHOTS}/04-office-view.png`,
    });

    await page
      .getByTestId("sidebar-primary-menu")
      .getByText("Inbox", { exact: true })
      .click();
    await expect(page.getByTestId("home-inbox-list")).toBeVisible();
    const hammer = page
      .locator("[data-testid^='mission-inbox-workbench-']")
      .first();
    await expect(hammer).toBeVisible();
    await hammer.click();
    await expect(page.getByTestId("workbench-thread")).toBeVisible();
    const questionCard = page.getByTestId(
      `channel-user-input-card-${REQUEST_ID}`,
    );
    await expect(questionCard).toBeVisible();
    await questionCard.scrollIntoViewIfNeeded();

    await page.getByRole("radio", { name: "Yes" }).click();
    await page.getByTestId("channel-user-input-submit").click();
    await expect
      .poll(async () =>
        page.evaluate(() =>
          (window.__BUZZ_E2E_COMMAND_PAYLOADS__ ?? []).some(
            (entry) => entry.command === "send_channel_user_input_answer",
          ),
        ),
      )
      .toBe(true);

    await page.getByTestId("workbench-open-channel").click();
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();

    await waitForLive(page, "engineering");
    await page.evaluate(
      ({ content, id, parentEventId, pubkey }) => {
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "engineering",
          content,
          createdAt: Math.floor(Date.now() / 1000) + 5,
          id,
          parentEventId,
          pubkey,
        });
      },
      {
        content: "NEW catch-up line after you left",
        id: CATCH_UP_ID,
        parentEventId: ROOT_A,
        pubkey: HERMES,
      },
    );
    await page.getByTestId("open-thread-workbench").click();
    await expect(page.getByTestId("workbench-catch-up")).toBeVisible();
    await expect(
      page.getByText("NEW catch-up line after you left"),
    ).toBeVisible();
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
            kind: "thread_workspace_ready",
            payload: {
              baseRevision: "abc123",
              baseSource: "remote",
              branch: "buzz/fix-reconnect",
              commitsBehindRemote: 0,
              remoteDefaultBranch: "main",
              repositoryPath: "/tmp/crew",
              rootEventId: "1".repeat(64),
              worktreeName: "crew-aaaaaaaaaaaa",
              worktreePath: "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
            },
            seq: 1,
            sessionId: "wb-session",
            timestamp: now,
            turnId: "wb-turn",
          },
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
          {
            agentIndex: 0,
            channelId: id,
            kind: "acp_read",
            payload: {
              method: "session/update",
              params: {
                sessionId: "wb-session",
                update: {
                  content: {
                    text: "ROLE-CHECK confirming owner before the edit",
                    type: "text",
                  },
                  sessionUpdate: "agent_thought_chunk",
                },
              },
            },
            seq: 3,
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
