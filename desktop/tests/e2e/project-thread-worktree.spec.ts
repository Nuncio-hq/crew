import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const ROOT_A = "a".repeat(64);
const ROOT_B = "b".repeat(64);
const CHANNEL_ID = "c0a5e9cd-e3b8-5d3e-83bd-2fe5d71980c8";
const agents = [
  {
    pubkey: TEST_IDENTITIES.alice.pubkey,
    name: "Claude Opus",
    status: "running" as const,
    channelNames: ["general"],
  },
  {
    pubkey: TEST_IDENTITIES.bob.pubkey,
    name: "Cursor Grok High Fast",
    status: "running" as const,
    channelNames: ["general"],
  },
  {
    pubkey: TEST_IDENTITIES.charlie.pubkey,
    name: "Codex GPT 5.6",
    status: "running" as const,
    channelNames: ["general"],
  },
];

test.use({ viewport: { height: 750, width: 1200 } });

async function waitForLiveChannel(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
            channelName: "general",
          }) ?? false,
      ),
    )
    .toBe(true);
}

async function emitProjectRoot(page: Page, id: string, label: string) {
  return page.evaluate(
    ({ alice, bob, charlie, eventId, taskLabel }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content:
          `[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew>\n\n` +
          `@Claude Opus ${taskLabel}, hand off to @Cursor Grok High Fast, then @Codex GPT 5.6 reviews.`,
        mentionPubkeys: [alice],
        extraTags: [
          ["mention", bob],
          ["mention", charlie],
        ],
        id: eventId,
      }),
    {
      alice: TEST_IDENTITIES.alice.pubkey,
      bob: TEST_IDENTITIES.bob.pubkey,
      charlie: TEST_IDENTITIES.charlie.pubkey,
      eventId: id,
      taskLabel: label,
    },
  );
}

async function openThread(page: Page, text: string) {
  const row = page.getByTestId("message-row").filter({ hasText: text });
  await expect(row).toBeVisible();
  await row.hover();
  await row.getByRole("button", { name: "Reply" }).click();
  const panel = page.getByTestId("message-thread-panel");
  await expect(panel).toBeVisible();
  return panel;
}

async function seedWorkspace(
  page: Page,
  rootEventId: string,
  conversationId: string,
  branch: string,
  path: string,
) {
  await page.evaluate(
    ({
      agentPubkey,
      branchName,
      channelId,
      conversation,
      root,
      worktreePath,
    }) => {
      const now = new Date().toISOString();
      window.__BUZZ_E2E_SEED_OBSERVER_EVENTS__?.({
        agentPubkey,
        events: [
          {
            seq: 1,
            timestamp: now,
            kind: "turn_started",
            agentIndex: 0,
            channelId,
            conversationId: conversation,
            sessionId: null,
            turnId: `turn-${root.slice(0, 8)}`,
            payload: {},
          },
          {
            seq: 2,
            timestamp: now,
            kind: "thread_workspace_ready",
            agentIndex: 0,
            channelId,
            conversationId: conversation,
            sessionId: null,
            turnId: `turn-${root.slice(0, 8)}`,
            payload: {
              rootEventId: root,
              branch: branchName,
              worktreePath,
              worktreeName: worktreePath.split("/").at(-1),
              baseRevision: "2e94a442f54a",
            },
          },
        ],
      });
    },
    {
      agentPubkey: TEST_IDENTITIES.alice.pubkey,
      branchName: branch,
      channelId: CHANNEL_ID,
      conversation: conversationId,
      root: rootEventId,
      worktreePath: path,
    },
  );
}

async function seedWorkspaceError(
  page: Page,
  rootEventId: string,
  conversationId: string,
  message: string,
) {
  await page.evaluate(
    ({ agentPubkey, channelId, conversation, errorMessage, root }) => {
      window.__BUZZ_E2E_SEED_OBSERVER_EVENTS__?.({
        agentPubkey,
        events: [
          {
            seq: 3,
            timestamp: new Date().toISOString(),
            kind: "thread_workspace_error",
            agentIndex: 0,
            channelId,
            conversationId: conversation,
            sessionId: null,
            turnId: `turn-${root.slice(0, 8)}`,
            payload: { rootEventId: root, message: errorMessage },
          },
        ],
      });
    },
    {
      agentPubkey: TEST_IDENTITIES.alice.pubkey,
      channelId: CHANNEL_ID,
      conversation: conversationId,
      errorMessage: message,
      root: rootEventId,
    },
  );
}

test("Project threads show truthful isolated workspace and agent handoff", async ({
  page,
}) => {
  await installMockBridge(page, { managedAgents: agents });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await emitProjectRoot(page, ROOT_A, "plan issue 4");
  let panel = await openThread(page, "plan issue 4");
  await expect(
    panel.getByTestId("project-thread-workspace-panel"),
  ).toContainText("Preparing isolated workspace");

  await seedWorkspace(
    page,
    ROOT_A,
    "conversation-a",
    "buzz/aaaaaaaaaaaa",
    "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
  );
  await expect(panel.getByText("Shared workspace ready")).toBeVisible();
  await expect(panel).toContainText("buzz/aaaaaaaaaaaa");
  await expect(panel).toContainText("Handoff in this thread");
  await expect(panel).toContainText("Claude Opus");
  await expect(panel).toContainText("Cursor Grok High Fast");
  await expect(panel).toContainText("Codex GPT 5.6");
  await waitForAnimations(page);
  await panel.screenshot({
    path: "test-results/thread-worktree/01-workspace-ready.png",
  });
  await page.screenshot({
    path: "test-results/thread-worktree/02-full-project-thread.png",
  });

  await panel.getByRole("button", { name: "Close panel" }).click();
  await emitProjectRoot(page, ROOT_B, "fix release updater");
  panel = await openThread(page, "fix release updater");
  await seedWorkspace(
    page,
    ROOT_B,
    "conversation-b",
    "buzz/bbbbbbbbbbbb",
    "/tmp/.buzz-worktrees/crew-bbbbbbbbbbbb",
  );
  await expect(panel).toContainText("buzz/bbbbbbbbbbbb");
  await expect(panel).not.toContainText("buzz/aaaaaaaaaaaa");
});

test("Project workspace errors render failed truth without preparing affordances", async ({
  page,
}) => {
  await installMockBridge(page, { managedAgents: agents });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await emitProjectRoot(page, ROOT_A, "reproduce setup failure");
  const panel = await openThread(page, "reproduce setup failure");
  await seedWorkspaceError(
    page,
    ROOT_A,
    "conversation-error",
    "branch already checked out",
  );

  await expect(
    panel.getByText("Workspace setup failed", { exact: true }),
  ).toBeVisible();
  await expect(panel.getByText("Failed", { exact: true })).toBeVisible();
  await expect(panel).toContainText("workspace setup failed");
  await expect(panel).toContainText("branch already checked out");
  await expect(panel.getByText("Preparing", { exact: true })).toHaveCount(0);
  await expect(panel).not.toContainText("preparing workspace");
  await expect(panel).not.toContainText("shared thread worktree");
});
