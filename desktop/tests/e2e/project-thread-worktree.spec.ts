import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const ROOT_A = "a".repeat(64);
const ROOT_B = "b".repeat(64);
// The real UUID of the mock bridge's `general` channel
// (STARTER_GENERAL_CHANNEL_ID in e2eBridge.ts). The archive read path
// (`read_archived_observer_events_for_channel`) is keyed by the channel the
// panel is actually open on, so fixtures MUST carry this id — a made-up UUID
// silently returns an empty archive and history mode never appears.
const CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
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
const archivedPeekEvents = [
  {
    id: "e2e-archive-peek-thought",
    pubkey: TEST_IDENTITIES.alice.pubkey,
    created_at: 1_754_000_003,
    kind: 24200,
    tags: [
      ["agent", TEST_IDENTITIES.alice.pubkey],
      ["frame", "telemetry"],
    ],
    content: JSON.stringify({
      seq: 3,
      timestamp: "2026-07-31T05:00:03.000Z",
      kind: "acp_read",
      agentIndex: 0,
      channelId: CHANNEL_ID,
      conversationId: "conversation-issue-82",
      sessionId: "session-issue-82",
      turnId: "turn-aaaaaaaa",
      payload: {
        method: "session/update",
        params: {
          sessionId: "session-issue-82",
          update: {
            sessionUpdate: "agent_thought_chunk",
            content: {
              type: "text",
              text: "Tracing the workspace projection.",
            },
          },
        },
      },
    }),
    sig: "",
  },
  {
    id: "e2e-archive-peek-tool-start",
    pubkey: TEST_IDENTITIES.alice.pubkey,
    created_at: 1_754_000_004,
    kind: 24200,
    tags: [
      ["agent", TEST_IDENTITIES.alice.pubkey],
      ["frame", "telemetry"],
    ],
    content: JSON.stringify({
      seq: 4,
      timestamp: "2026-07-31T05:00:04.000Z",
      kind: "acp_read",
      agentIndex: 0,
      channelId: CHANNEL_ID,
      conversationId: "conversation-issue-82",
      sessionId: "session-issue-82",
      turnId: "turn-aaaaaaaa",
      payload: {
        method: "session/update",
        params: {
          sessionId: "session-issue-82",
          update: {
            sessionUpdate: "tool_call",
            toolCallId: "call-check",
            status: "executing",
            title: "shell",
            kind: "shell",
            rawInput: { command: "pnpm run check" },
          },
        },
      },
    }),
    sig: "",
  },
  {
    id: "e2e-archive-peek-tool-result",
    pubkey: TEST_IDENTITIES.alice.pubkey,
    created_at: 1_754_000_005,
    kind: 24200,
    tags: [
      ["agent", TEST_IDENTITIES.alice.pubkey],
      ["frame", "telemetry"],
    ],
    content: JSON.stringify({
      seq: 5,
      timestamp: "2026-07-31T05:00:05.000Z",
      kind: "acp_read",
      agentIndex: 0,
      channelId: CHANNEL_ID,
      conversationId: "conversation-issue-82",
      sessionId: "session-issue-82",
      turnId: "turn-aaaaaaaa",
      payload: {
        method: "session/update",
        params: {
          sessionId: "session-issue-82",
          update: {
            sessionUpdate: "tool_call_update",
            toolCallId: "call-check",
            status: "completed",
            title: "shell",
            kind: "shell",
            rawOutput: "All checks passed",
          },
        },
      },
    }),
    sig: "",
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

async function seedPeekActivity(
  page: Page,
  conversationId: string,
  terminal = false,
  archive = false,
) {
  await page.evaluate(
    ({ agentPubkey, channelId, conversation, endTurn, archive }) => {
      const event = (seq: number, update: Record<string, unknown>) => ({
        seq,
        timestamp: new Date(Date.now() + seq).toISOString(),
        kind: "acp_read",
        agentIndex: 0,
        channelId,
        conversationId: conversation,
        sessionId: "session-issue-82",
        turnId: "turn-aaaaaaaa",
        payload: {
          method: "session/update",
          params: { sessionId: "session-issue-82", update },
        },
      });
      const events = endTurn
        ? [
            {
              seq: 7,
              timestamp: new Date(Date.now() + 7).toISOString(),
              kind: "turn_completed",
              agentIndex: 0,
              channelId,
              conversationId: conversation,
              sessionId: "session-issue-82",
              turnId: "turn-aaaaaaaa",
              payload: {},
            },
          ]
        : [
            event(3, {
              sessionUpdate: "agent_thought_chunk",
              content: {
                type: "text",
                text: "Tracing the workspace projection.",
              },
            }),
            event(4, {
              sessionUpdate: "tool_call",
              toolCallId: "call-check",
              status: "executing",
              title: "shell",
              kind: "shell",
              rawInput: { command: "pnpm run check" },
            }),
            event(5, {
              sessionUpdate: "tool_call_update",
              toolCallId: "call-check",
              status: "completed",
              title: "shell",
              kind: "shell",
              rawOutput: "All checks passed",
            }),
          ];
      window.__BUZZ_E2E_SEED_OBSERVER_EVENTS__?.({
        agentPubkey,
        archive,
        events,
      });
    },
    {
      agentPubkey: TEST_IDENTITIES.alice.pubkey,
      channelId: CHANNEL_ID,
      conversation: conversationId,
      endTurn: terminal,
      archive,
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

  await panel.getByRole("button", { name: "Close details" }).click();
  const githubInvokesBeforePr = await countGitHubStatusInvokes(page);
  await panel.getByTestId("thread-forge-summary-card").click();
  const hub = page
    .getByTestId("channel-tool-pane")
    .getByTestId("thread-pr-hub");
  await expect(hub).toBeVisible();
  await expect(page.getByTestId("thread-pr-hub-changes")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_COMMAND_LOG__
            ?.filter((entry) => entry.command === "get_thread_forge_pr_detail")
            .at(-1)?.payload,
      ),
    )
    .toMatchObject({ owner: "Nuncio-hq", name: "crew", number: 9 });
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
  await hub.screenshot({
    path: "test-results/thread-worktree/02-pr-history.png",
  });
  const chatToggle = page
    .getByTestId("thread-forge-pane-toggle")
    .getByRole("button", { name: "Chat", exact: true });
  if (await chatToggle.isVisible()) await chatToggle.click();
  await page
    .getByRole("button", { name: "Show thread beside channel" })
    .click();
  await expect(panel).toBeVisible();
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
    panel.getByRole("button", { name: "Free local space" }),
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

test("Docked thread panel can expand to show worktree branch detail", async ({
  page,
}) => {
  await installMockBridge(page, { managedAgents: agents });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await emitProjectRoot(page, ROOT_A, "expand docked workspace detail");
  const panel = await openThread(page, "expand docked workspace detail");
  await seedWorkspace(
    page,
    ROOT_A,
    "conversation-docked-expand",
    "buzz/aaaaaaaaaaaa",
    "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
  );

  // Side panel is not focus mode — expand must still reveal the grid.
  await expect(page.getByTestId("focus-thread-drawer")).toHaveCount(0);
  // Docked panel defaults to collapsed (compact is the default).
  await expect(panel.getByTestId("project-thread-status-expanded")).toHaveCount(
    0,
  );
  await panel.getByTestId("project-thread-status-expand").click();
  const expanded = panel.getByTestId("project-thread-status-expanded");
  await expect(expanded).toBeVisible();
  await expect(expanded).toContainText("buzz/aaaaaaaaaaaa");
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
    "GitHub CLI (gh) not found. Click to retry.",
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
  // lives in the expanded grid — reachable from the side panel now.
  await panel.getByRole("button", { name: /Workspace/ }).click();
  await expect(panel).toContainText("branch already checked out");
  await expect(panel).not.toContainText("Preparing");
  await expect(panel).not.toContainText("preparing this isolated worktree");
  await panel.getByRole("button", { name: "Close details" }).click();

  await panel.getByTestId("project-thread-status-expand").click();
  await expect(
    panel.getByTestId("project-thread-status-expanded"),
  ).toContainText("Setup failed");
});

test("Project thread phase chips and transcript peek stay reviewable after completion", async ({
  page,
}) => {
  await installMockBridge(page, {
    managedAgents: agents,
    saveSubscriptions: [
      {
        scope_type: "owner_p",
        // MUST be the signed-in identity's pubkey: useLoadArchivedObserverEvents
        // gates the entire archive hydration path on
        // `scopeValue === identityPubkey` (tyler is the default mock identity).
        scope_value: TEST_IDENTITIES.tyler.pubkey,
        kinds: "[24200]",
      },
    ],
    threadGitHubByBranch: {
      "buzz/aaaaaaaaaaaa": threadPullRequest,
    },
    archivedObserverEvents: archivedPeekEvents,
  });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await emitProjectRoot(page, ROOT_A, "show mission control activity");
  let panel = await openThread(page, "show mission control activity");
  await seedWorkspace(
    page,
    ROOT_A,
    "conversation-issue-82",
    "buzz/aaaaaaaaaaaa",
    "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
  );
  await seedPeekActivity(page, "conversation-issue-82");

  const peek = panel.getByTestId("project-thread-activity-peek");
  await expect(peek).toHaveAttribute("data-mode", "live");
  await expect(peek).toContainText("Claude Opus");
  await peek.getByTestId("project-thread-peek-toggle").click();
  await expect(peek.getByTestId("project-thread-peek-thinking")).toContainText(
    "Tracing the workspace projection.",
  );
  await expect(
    peek.getByTestId("project-thread-peek-tool-result"),
  ).toContainText("All checks passed");
  await waitForAnimations(page);
  await peek.screenshot({
    path: "test-results/issue-82/01-live-peek.png",
  });

  const statusBar = panel.getByTestId("project-thread-workspace-panel");
  await expect(
    statusBar
      .getByRole("button", { name: /Task/ })
      .getByTestId("project-thread-phase-dot"),
  ).toHaveAttribute("data-phase", "complete");
  await expect(
    statusBar
      .getByRole("button", { name: /Workspace/ })
      .getByTestId("project-thread-phase-dot"),
  ).toHaveAttribute("data-phase", "complete");
  await expect(
    statusBar
      .getByRole("button", { name: /PR/ })
      .getByTestId("project-thread-phase-dot"),
  ).toHaveAttribute("data-phase", "active");
  await expect(
    statusBar
      .getByRole("button", { name: /CI/ })
      .getByTestId("project-thread-phase-dot"),
  ).toHaveAttribute("data-phase", "active");
  await waitForAnimations(page);
  await statusBar.screenshot({
    path: "test-results/issue-82/02-chip-matrix.png",
  });

  await page.evaluate(() => window.__BUZZ_E2E_RESET_OBSERVER_EVENTS__?.());
  await seedPeekActivity(page, "conversation-issue-82", true);
  await page.getByTestId("channel-random").click();
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);
  panel = await openThread(page, "show mission control activity");
  const historyPeek = panel.getByTestId("project-thread-activity-peek");
  await expect(historyPeek).toHaveAttribute("data-mode", "history");
  await expect(historyPeek).toContainText("History");
  // The remounted panel starts collapsed — expand to reveal the archived feed.
  await historyPeek.getByTestId("project-thread-peek-toggle").click();
  await expect(historyPeek).toContainText("Tracing the workspace projection.");
  await waitForAnimations(page);
  await historyPeek.screenshot({
    path: "test-results/issue-82/03-history-peek.png",
  });
});
