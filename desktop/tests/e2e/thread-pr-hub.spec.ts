import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const BRANCH = "buzz/cccccccccccc";
const AGENT = TEST_IDENTITIES.alice.pubkey;
const OWNER = "deadbeef".repeat(8);
const SHOTS = "test-results/thread-pr-hub";
const PR_URL = "https://github.com/Nuncio-hq/crew/pull/193";

function rootId(nibble: string) {
  const tag = nibble
    .replace(/[^0-9a-f]/gi, "")
    .toLowerCase()
    .padStart(2, "0");
  return `${"c".repeat(62)}${tag.slice(0, 2)}`;
}

function githubStatus(
  extras: {
    isDraft?: boolean;
    state?: string;
    reviewDecision?: string;
    checks?: Array<{ name: string; state: string }>;
    title?: string;
  } = {},
) {
  return {
    availability: "available" as const,
    pullRequest: {
      additions: 42,
      baseRefName: "main",
      changedFiles: 2,
      checks: (
        extras.checks ?? [{ name: "NuncioCrew Gate", state: "SUCCESS" }]
      ).map((check) => ({
        name: check.name,
        state: check.state,
        url: null,
        workflow: "CI",
      })),
      closingIssuesReferences: [],
      comments: [],
      deletions: 7,
      headRefName: BRANCH,
      isDraft: extras.isDraft ?? false,
      mergeStateStatus: "CLEAN",
      number: 193,
      reviewDecision: extras.reviewDecision ?? "CHANGES_REQUESTED",
      state: extras.state ?? "OPEN",
      title: extras.title ?? "GitHub PR hub in thread focus",
      url: PR_URL,
    },
  };
}

async function waitForLiveChannel(page: Page) {
  await page.waitForFunction(
    () =>
      window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
        channelName: "general",
      }) === true,
  );
}

async function openProjectThread(
  page: Page,
  root: string,
  options: {
    canvasHeld?: boolean;
    github?:
      | ReturnType<typeof githubStatus>
      | { availability: string; pullRequest: null };
  } = {},
) {
  await installMockBridge(page, {
    searchProfiles: [
      {
        pubkey: AGENT,
        displayName: "Reviewer Agent",
        ownerPubkey: OWNER,
        isAgent: true,
      },
    ],
    managedAgents: [
      {
        pubkey: AGENT,
        name: "Reviewer Agent",
        status: "running",
        channelNames: ["general"],
      },
    ],
    threadGitHubByBranch: {
      [BRANCH]: options.github ?? githubStatus(),
    },
    canvas: options.canvasHeld
      ? {
          routing: [
            {
              work_type: "code-review",
              role_label: "Reviewer",
              holders: [AGENT],
              unheld_message: null,
            },
          ],
          assignments: [],
        }
      : {
          routing: [],
          assignments: [],
        },
  } as never);
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await page.evaluate(
    ({ agent, eventId }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content:
          `[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew>\n\n` +
          `@Reviewer Agent review the pull request.`,
        mentionPubkeys: [agent],
        id: eventId,
      }),
    { agent: AGENT, eventId: root },
  );

  const row = page
    .getByTestId("message-row")
    .filter({ hasText: "review the pull request" });
  await expect(row).toBeVisible();
  await row.hover();
  await row.getByRole("button", { name: "Reply" }).click();
  const panel = page.getByTestId("message-thread-panel");
  await expect(panel).toBeVisible();

  await page.evaluate(
    ({ agentPubkey, branchName, channelId, conversation, rootEventId }) => {
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
            turnId: `turn-${rootEventId.slice(0, 8)}`,
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
            turnId: `turn-${rootEventId.slice(0, 8)}`,
            payload: {
              rootEventId,
              branch: branchName,
              worktreePath: "/tmp/.buzz-worktrees/crew-cccccccccccc",
              worktreeName: "crew-cccccccccccc",
              baseRevision: "2e94a442f54a",
              baseSource: "remote",
              commitsBehindRemote: 0,
              remoteDefaultBranch: "main",
              repositoryPath: "/tmp/crew",
            },
          },
        ],
      });
    },
    {
      agentPubkey: AGENT,
      branchName: BRANCH,
      channelId: CHANNEL_ID,
      conversation: `conversation-${root.slice(0, 12)}`,
      rootEventId: root,
    },
  );

  await expect(panel.getByText("Ready", { exact: true })).toBeVisible({
    timeout: 15_000,
  });
  return panel;
}

test.describe("GitHub PR hub in thread focus (#193)", () => {
  test("Tier-1 card states and click opens focus hub", async ({ page }) => {
    const root = rootId("01");
    const panel = await openProjectThread(page, root);
    const card = panel.getByTestId("thread-forge-summary-card");
    await expect(card).toBeVisible();
    await expect(card).toContainText("Open");
    await expect(card).toContainText("changes requested");
    await waitForAnimations(page);
    await card.screenshot({ path: `${SHOTS}/01-card-open.png` });

    await page.evaluate((branch) => {
      window.__BUZZ_E2E_SET_THREAD_GITHUB_BY_BRANCH__?.(branch, {
        availability: "available",
        pullRequest: {
          additions: 1,
          baseRefName: "main",
          changedFiles: 1,
          checks: [{ name: "CI", state: "FAILURE", url: null, workflow: "CI" }],
          closingIssuesReferences: [],
          comments: [],
          deletions: 0,
          headRefName: branch,
          isDraft: true,
          mergeStateStatus: "CLEAN",
          number: 193,
          reviewDecision: "REVIEW_REQUIRED",
          state: "OPEN",
          title: "Draft hub",
          url: "https://github.com/Nuncio-hq/crew/pull/193",
        },
      });
    }, BRANCH);
    await expect(card).toContainText("Draft");
    await waitForAnimations(page);
    await card.screenshot({ path: `${SHOTS}/02-card-draft.png` });

    await page.evaluate((branch) => {
      window.__BUZZ_E2E_SET_THREAD_GITHUB_BY_BRANCH__?.(branch, {
        availability: "available",
        pullRequest: {
          additions: 1,
          baseRefName: "main",
          changedFiles: 1,
          checks: [],
          closingIssuesReferences: [],
          comments: [],
          deletions: 0,
          headRefName: branch,
          isDraft: false,
          mergeStateStatus: "UNKNOWN",
          number: 193,
          reviewDecision: "APPROVED",
          state: "MERGED",
          title: "Merged hub",
          url: "https://github.com/Nuncio-hq/crew/pull/193",
        },
      });
    }, BRANCH);
    await expect(card).toContainText("Merged");
    await waitForAnimations(page);
    await card.screenshot({ path: `${SHOTS}/03-card-merged.png` });

    await page.evaluate((branch) => {
      window.__BUZZ_E2E_SET_THREAD_GITHUB_BY_BRANCH__?.(branch, {
        availability: "available",
        pullRequest: {
          additions: 1,
          baseRefName: "main",
          changedFiles: 1,
          checks: [],
          closingIssuesReferences: [],
          comments: [],
          deletions: 0,
          headRefName: branch,
          isDraft: false,
          mergeStateStatus: "UNKNOWN",
          number: 193,
          reviewDecision: "",
          state: "CLOSED",
          title: "Closed hub",
          url: "https://github.com/Nuncio-hq/crew/pull/193",
        },
      });
    }, BRANCH);
    await expect(card).toContainText("Closed");
    await waitForAnimations(page);
    await card.screenshot({ path: `${SHOTS}/04-card-closed.png` });

    await card.click();
    await expect(page.getByTestId("focus-thread-drawer")).toBeVisible();
    await expect(page.getByTestId("thread-pr-hub")).toBeVisible();
    await expect(page.getByTestId("thread-pr-hub-changes")).toBeVisible();
  });

  test("hub tabs, two inputs, checks, and narrow toggle", async ({ page }) => {
    const root = rootId("02");
    const panel = await openProjectThread(page, root, { canvasHeld: true });
    await panel.getByTestId("thread-forge-summary-card").click();
    const hub = page.getByTestId("thread-pr-hub");
    await expect(hub).toBeVisible();

    await hub.getByRole("tab", { name: /Description/ }).click();
    await expect(page.getByTestId("thread-pr-hub-description")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-description")
      .screenshot({ path: `${SHOTS}/05-tab-description.png` });

    await hub.getByRole("tab", { name: /Discussion/ }).click();
    await expect(page.getByTestId("thread-pr-hub-discussion")).toBeVisible();
    await page
      .getByTestId("thread-pr-hub-github-comment")
      .fill("Seen on GitHub");
    await page.getByTestId("thread-pr-hub-github-comment-submit").click();
    await expect
      .poll(async () =>
        page.evaluate(() =>
          (window.__BUZZ_E2E_COMMANDS__ ?? []).includes("comment_forge_pr"),
        ),
      )
      .toBe(true);

    const threadInput = page
      .getByTestId("message-thread-panel")
      .getByTestId("message-input");
    await threadInput.fill("Room-only note");
    await page
      .getByTestId("message-thread-panel")
      .getByTestId("send-message")
      .click();
    await expect(
      page.getByTestId("message-thread-panel").getByText("Room-only note"),
    ).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-discussion")
      .screenshot({ path: `${SHOTS}/06-tab-discussion.png` });

    await hub.getByRole("tab", { name: /Commits/ }).click();
    await expect(page.getByTestId("thread-pr-hub-commits")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-commits")
      .screenshot({ path: `${SHOTS}/07-tab-commits.png` });

    await hub.getByRole("tab", { name: /Checks/ }).click();
    await expect(page.getByTestId("thread-pr-hub-checks")).toBeVisible();
    await page.getByTestId("thread-pr-hub-check-Desktop Fast").click();
    await expect(page.getByTestId("thread-pr-hub-check-log")).toContainText(
      "##[error]",
    );
    await page.getByTestId("thread-pr-hub-rerun-failed").click();
    await expect
      .poll(async () =>
        page.evaluate(() =>
          (window.__BUZZ_E2E_COMMANDS__ ?? []).includes("rerun_forge_checks"),
        ),
      )
      .toBe(true);
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-checks")
      .screenshot({ path: `${SHOTS}/08-tab-checks.png` });

    await hub.getByRole("tab", { name: /Changes/ }).click();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-changes")
      .screenshot({ path: `${SHOTS}/09-tab-changes.png` });

    await page.setViewportSize({ width: 720, height: 720 });
    await expect(page.getByTestId("thread-forge-pane-toggle")).toBeVisible();
    await page.getByRole("button", { name: "Chat" }).click();
    await expect(page.getByTestId("thread-pr-hub")).toBeHidden();
    await page.getByRole("button", { name: "PR" }).click();
    await expect(page.getByTestId("thread-pr-hub")).toBeVisible();
  });

  test("Bugs tab resolves Reviewer or shows picker", async ({ page }) => {
    const heldRoot = rootId("03");
    const heldPanel = await openProjectThread(page, heldRoot, {
      canvasHeld: true,
    });
    await heldPanel.getByTestId("thread-forge-summary-card").click();
    const hub = page.getByTestId("thread-pr-hub");
    await hub.getByRole("tab", { name: /Bugs/ }).click();
    await expect(page.getByTestId("thread-pr-hub-reviewer")).toBeVisible();
    await page.getByTestId("thread-pr-hub-run-analysis").click();
    await expect(
      page.getByTestId("message-thread-panel").getByText("please review"),
    ).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-bugs")
      .screenshot({ path: `${SHOTS}/10-tab-bugs-held.png` });

    await page.evaluate(
      ({ pubkey, rootEventId }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "general",
          content: "Null unwrap in hub.ts",
          pubkey,
          parentEventId: rootEventId,
          extraTags: [["crew-finding", "error", "desktop/src/hub.ts", "12-14"]],
          id: `${"d".repeat(62)}01`,
        }),
      { pubkey: AGENT, rootEventId: heldRoot },
    );
    await expect(page.getByTestId("thread-pr-hub-bugs")).toContainText(
      "hub.ts",
    );
  });

  test("Bugs picker when Reviewer is unheld", async ({ page }) => {
    const root = rootId("04");
    const panel = await openProjectThread(page, root, { canvasHeld: false });
    await panel.getByTestId("thread-forge-summary-card").click();
    await page
      .getByTestId("thread-pr-hub")
      .getByRole("tab", { name: /Bugs/ })
      .click();
    await expect(
      page.getByTestId("thread-pr-hub-reviewer-picker"),
    ).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-bugs")
      .screenshot({ path: `${SHOTS}/11-tab-bugs-picker.png` });
  });

  test("PR-by-URL opens the hub from a plain link", async ({ page }) => {
    const root = rootId("05");
    const panel = await openProjectThread(page, root);
    await page.evaluate(
      ({ rootEventId }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "general",
          content: "See https://github.com/Nuncio-hq/crew/pull/196",
          parentEventId: rootEventId,
          id: `${"e".repeat(62)}01`,
        }),
      { rootEventId: root },
    );
    const openHub = panel.getByTestId("open-pr-hub").last();
    await expect(openHub).toBeVisible();
    await openHub.click();
    await expect(page.getByTestId("focus-thread-drawer")).toBeVisible();
    await expect(page.getByTestId("thread-pr-hub")).toBeVisible();
  });

  test("degraded cli-missing and rate-limited states", async ({ page }) => {
    const root = rootId("06");
    const panel = await openProjectThread(page, root);
    await page.evaluate(() => {
      window.__BUZZ_E2E_SET_FORGE_PR_DETAIL__?.({
        availability: "cli-missing",
        message: "Forge CLI was not found.",
        detail: null,
      });
    });
    await panel.getByTestId("thread-forge-summary-card").click();
    await expect(page.getByTestId("thread-pr-hub-cli-missing")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-cli-missing")
      .screenshot({ path: `${SHOTS}/12-degraded-cli-missing.png` });

    await page.evaluate(() => {
      window.__BUZZ_E2E_SET_FORGE_PR_DETAIL__?.({
        availability: "rate-limited",
        rateLimitedUntil: "2099-01-01T00:00:00Z",
        message: "rate limited",
        detail: null,
      });
    });
    await page.getByTestId("thread-pr-hub-recheck").click();
    await expect(page.getByTestId("thread-pr-hub-rate-limited")).toBeVisible();
    await waitForAnimations(page);
    await page
      .getByTestId("thread-pr-hub-rate-limited")
      .screenshot({ path: `${SHOTS}/13-degraded-rate-limited.png` });
  });
});
