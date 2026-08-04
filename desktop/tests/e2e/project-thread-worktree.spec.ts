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
const threadPullRequest = {
  availability: "available" as const,
  pullRequest: {
    additions: 214,
    baseRefName: "main",
    changedFiles: 12,
    checks: [
      {
        name: "Crew CI",
        state: "SUCCESS",
        url: "https://github.com/Nuncio-hq/crew/actions/runs/1",
        workflow: "Crew CI",
      },
      {
        name: "Desktop E2E",
        state: "IN_PROGRESS",
        url: "https://github.com/Nuncio-hq/crew/actions/runs/2",
        workflow: "Crew CI",
      },
    ],
    closingIssuesReferences: [
      {
        number: 12,
        state: "OPEN",
        title: "Integrate Project thread lifecycle",
        url: "https://github.com/Nuncio-hq/crew/issues/12",
      },
    ],
    comments: [
      {
        author: { login: "oscarlehuu" },
        body: "Keep the 2×3 layout and existing app colors.",
        createdAt: "2026-07-31T05:00:00Z",
        url: "https://github.com/Nuncio-hq/crew/pull/9#issuecomment-1",
      },
    ],
    deletions: 32,
    headRefName: "buzz/aaaaaaaaaaaa",
    isDraft: false,
    mergeStateStatus: "CLEAN",
    number: 9,
    reviewDecision: "REVIEW_REQUIRED",
    state: "OPEN",
    title: "Thread integration strip",
    url: "https://github.com/Nuncio-hq/crew/pull/9",
  },
};

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
    ({ alice, charlie, eventId, taskLabel }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content:
          `[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew>\n\n` +
          `@Claude Opus ${taskLabel}, then @Codex GPT 5.6 reviews.`,
        mentionPubkeys: [alice],
        extraTags: [["mention", charlie]],
        id: eventId,
      }),
    {
      alice: TEST_IDENTITIES.alice.pubkey,
      charlie: TEST_IDENTITIES.charlie.pubkey,
      eventId: id,
      taskLabel: label,
    },
  );
}

async function emitReplyMention(page: Page, rootEventId: string) {
  await page.evaluate(
    ({ bob, root }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: "Adding @Cursor Grok High Fast for implementation review.",
        mentionPubkeys: [bob],
        parentEventId: root,
      }),
    { bob: TEST_IDENTITIES.bob.pubkey, root: rootEventId },
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
  commitsBehindRemote = 0,
) {
  await page.evaluate(
    ({
      agentPubkey,
      behind,
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
              baseSource: "remote",
              commitsBehindRemote: behind,
              remoteDefaultBranch: "main",
              repositoryPath: "/tmp/crew",
            },
          },
        ],
      });
    },
    {
      agentPubkey: TEST_IDENTITIES.alice.pubkey,
      behind: commitsBehindRemote,
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

async function countGitHubStatusInvokes(page: Page) {
  return page.evaluate(
    () =>
      (
        window as Window & {
          __BUZZ_E2E_COMMAND_LOG__?: Array<{ command: string }>;
        }
      ).__BUZZ_E2E_COMMAND_LOG__?.filter(
        (entry) => entry.command === "get_thread_github_status",
      ).length ?? 0,
  );
}

test("Project threads show truthful isolated workspace and agent handoff", async ({
  page,
}) => {
  await installMockBridge(page, {
    managedAgents: agents,
    threadGitHubByBranch: {
      "buzz/aaaaaaaaaaaa": threadPullRequest,
    },
    threadWorkspaceDirtyByBranch: {
      "buzz/aaaaaaaaaaaa": true,
    },
  });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await emitProjectRoot(page, ROOT_A, "plan issue 4");
  let panel = await openThread(page, "plan issue 4");
  await panel.getByRole("button", { name: /Workspace/ }).click();
  await expect(
    panel.getByTestId("project-thread-workspace-panel"),
  ).toContainText("The harness is preparing this isolated worktree");

  await seedWorkspace(
    page,
    ROOT_A,
    "conversation-a",
    "buzz/aaaaaaaaaaaa",
    "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
  );
  await expect(panel.getByText("Ready", { exact: true })).toBeVisible();
  await expect(panel).toContainText("buzz/aaaaaaaaaaaa");

  const githubInvokesBeforePr = await countGitHubStatusInvokes(page);
  await panel.getByRole("button", { name: /^PR$/ }).click();
  await expect(panel).toContainText("Pull request");
  await expect(panel).toContainText(
    "Keep the 2×3 layout and existing app colors.",
  );
  await expect(panel).toContainText("#9 Thread integration strip");
  // #34: opening the PR drawer must not start an unbounded refresh loop.
  await expect
    .poll(() => countGitHubStatusInvokes(page))
    .toBeLessThanOrEqual(githubInvokesBeforePr + 2);
  const githubInvokesAfterPr = await countGitHubStatusInvokes(page);
  await page.evaluate(() => {
    // Force a parent re-render without closing the drawer.
    window.dispatchEvent(new Event("resize"));
  });
  await expect
    .poll(() => countGitHubStatusInvokes(page))
    .toBe(githubInvokesAfterPr);

  await waitForAnimations(page);
  await panel.screenshot({
    path: "test-results/thread-worktree/02-pr-history.png",
  });
  await panel.getByRole("button", { name: "Close details" }).click();
  await emitReplyMention(page, ROOT_A);
  await panel.getByRole("button", { name: /Handoff/ }).click();
  await expect(panel).toContainText("Handoff in this thread");
  await expect(panel).toContainText("Claude Opus");
  await expect(panel).toContainText("Cursor Grok High Fast");
  await expect(panel).toContainText("Codex GPT 5.6");
  await expect(panel).toContainText("Added in a reply");
  await panel.getByRole("button", { name: "Close details" }).click();
  await waitForAnimations(page);
  await panel.screenshot({
    path: "test-results/thread-worktree/01-integration-strip.png",
  });
  await panel.getByRole("button", { name: /Workspace/ }).click();
  await expect(
    panel.getByRole("button", { name: "Remove worktree" }),
  ).toBeDisabled();
  await expect(panel.getByRole("button", { name: /^PR$/ })).toBeVisible();
  await waitForAnimations(page);
  await panel.screenshot({
    path: "test-results/thread-worktree/03-workspace-ready.png",
  });
  await page.screenshot({
    path: "test-results/thread-worktree/04-full-project-thread.png",
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
    2,
  );
  await panel.getByRole("button", { name: /Workspace/ }).click();
  await expect(panel).toContainText("buzz/bbbbbbbbbbbb");
  await expect(panel).toContainText("2 behind origin/main");
  await expect(panel).toContainText("Remote base");
  await expect(panel).not.toContainText("buzz/aaaaaaaaaaaa");
  await expect(panel.getByRole("button", { name: /^PR$/ })).toHaveCount(0);
});

test("Degraded GitHub availability shows a muted chip, not silent empty", async ({
  page,
}) => {
  await installMockBridge(page, {
    managedAgents: agents,
    threadGitHubByBranch: {
      "buzz/aaaaaaaaaaaa": {
        availability: "cli-missing",
        pullRequest: null,
      },
    },
  });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await emitProjectRoot(page, ROOT_A, "diagnose missing gh");
  const panel = await openThread(page, "diagnose missing gh");
  await seedWorkspace(
    page,
    ROOT_A,
    "conversation-degraded-gh",
    "buzz/aaaaaaaaaaaa",
    "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
  );

  const githubChip = panel.getByRole("button", { name: /^GitHub$/ });
  await expect(githubChip).toBeVisible();
  await expect(githubChip).toHaveAttribute(
    "title",
    "GitHub CLI (gh) not found",
  );
  await expect(panel.getByRole("button", { name: /^PR$/ })).toHaveCount(0);
  await expect(panel.getByRole("button", { name: /^CI$/ })).toHaveCount(0);
});

test("Docked <h2> title fallback does not steal Workspace clicks (#31)", async ({
  page,
}) => {
  await installMockBridge(page, { managedAgents: agents });
  await page.goto("/");
  // Set after bridge init — it replaces window.__BUZZ_E2E__ on boot.
  await page.evaluate(() => {
    const w = window as Window & {
      __BUZZ_E2E__?: { forceThreadTitleFallback?: boolean };
    };
    w.__BUZZ_E2E__ = { ...w.__BUZZ_E2E__, forceThreadTitleFallback: true };
  });
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await emitProjectRoot(page, ROOT_A, "h2 title hit target");
  const panel = await openThread(page, "h2 title hit target");
  await expect(panel.getByTestId("thread-breadcrumb")).toHaveCount(0);
  await expect(panel.getByRole("heading", { name: "Thread" })).toBeVisible();

  // Plain click — no force. If the docked <h2> overlap returns, this fails.
  await panel.getByRole("button", { name: /Workspace/ }).click();
  await expect(
    panel.getByTestId("project-thread-workspace-panel"),
  ).toContainText("The harness is preparing this isolated worktree");
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

  // Docked sticky bar opens the drawer (error message only). "Setup failed"
  // lives in the focus-mode expanded grid — assert it there (option a).
  await panel.getByRole("button", { name: /Workspace/ }).click();
  await expect(panel).toContainText("branch already checked out");
  await expect(panel).not.toContainText("Preparing");
  await expect(panel).not.toContainText("preparing this isolated worktree");
  await panel.getByRole("button", { name: "Close details" }).click();

  await page.getByRole("button", { name: "Expand thread" }).click();
  const focusDrawer = page.getByTestId("focus-thread-drawer");
  await expect(focusDrawer).toBeVisible();
  await focusDrawer.getByTestId("project-thread-status-expand").click();
  // Option (a): "Setup failed" only lives in the focus-mode expanded grid cell.
  // The error message itself was already asserted on the docked drawer above.
  await expect(
    focusDrawer.getByTestId("project-thread-status-expanded"),
  ).toContainText("Setup failed");
});
