import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const BRANCH = "buzz/cccccccccccc";
const AGENT = TEST_IDENTITIES.alice.pubkey;
const OWNER = "deadbeef".repeat(8);

/** 64-char hex root ids — workspace ingest rejects non-hex. */
function rootId(nibble: string) {
  const tag = nibble
    .replace(/[^0-9a-f]/gi, "")
    .toLowerCase()
    .padStart(2, "0");
  return `${"c".repeat(62)}${tag.slice(0, 2)}`;
}

function basePr(
  checks: Array<{ name: string; state: string }>,
  extras: {
    additions?: number;
    deletions?: number;
    changedFiles?: number;
  } = {},
) {
  return {
    availability: "available" as const,
    pullRequest: {
      additions: extras.additions ?? 100,
      baseRefName: "main",
      changedFiles: extras.changedFiles ?? 5,
      checks: checks.map((check) => ({
        name: check.name,
        state: check.state,
        url: null,
        workflow: "CI",
      })),
      closingIssuesReferences: [],
      comments: [],
      deletions: extras.deletions ?? 20,
      headRefName: BRANCH,
      isDraft: false,
      mergeStateStatus: "CLEAN",
      number: 175,
      reviewDecision: "REVIEW_REQUIRED",
      state: "OPEN",
      title: "Evidence cross-check",
      url: "https://github.com/Nuncio-hq/crew/pull/175",
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
  github = basePr([{ name: "Crew CI", state: "SUCCESS" }]),
) {
  await installMockBridge(page, {
    searchProfiles: [
      {
        pubkey: AGENT,
        displayName: "Evidence Agent",
        ownerPubkey: OWNER,
        isAgent: true,
      },
    ],
    managedAgents: [
      {
        pubkey: AGENT,
        name: "Evidence Agent",
        status: "running",
        channelNames: ["general"],
      },
    ],
    threadGitHubByBranch: {
      [BRANCH]: github,
    },
  });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await waitForLiveChannel(page);

  await page.evaluate(
    ({ agent, eventId }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content:
          `[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew>\n\n` +
          `@Evidence Agent cross-check the evidence badge.`,
        mentionPubkeys: [agent],
        id: eventId,
      }),
    { agent: AGENT, eventId: root },
  );

  const row = page
    .getByTestId("message-row")
    .filter({ hasText: "cross-check the evidence badge" });
  await expect(row).toBeVisible();
  await row.hover();
  await row.getByRole("button", { name: "Reply" }).click();
  const panel = page.getByTestId("message-thread-panel");
  await expect(panel).toBeVisible();

  await panel.getByRole("button", { name: /Workspace/ }).click();
  await expect(
    panel.getByTestId("project-thread-workspace-panel"),
  ).toBeVisible();

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

async function emitEvidence(
  page: Page,
  root: string,
  content: string,
  kind: "test-run" | "diff-stat" | "metrics" | "before-after-visual",
) {
  return page.evaluate(
    ({ body, evidenceKind, pubkey, rootEventId }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: body,
        pubkey,
        parentEventId: rootEventId,
        extraTags: [["crew-evidence", evidenceKind]],
      }),
    { body: content, evidenceKind: kind, pubkey: AGENT, rootEventId: root },
  );
}

test.describe("evidence–CI cross-check badge (#175)", () => {
  test("Matches CI for green test-run claim", async ({ page }) => {
    const root = rootId("01");
    const panel = await openProjectThread(
      page,
      root,
      basePr([{ name: "Crew CI", state: "SUCCESS" }]),
    );
    await emitEvidence(page, root, "Tests: 14 passed, 0 failed", "test-run");
    const card = panel.getByTestId("evidence-card-test-run");
    await expect(card).toBeVisible();
    const badge = card.getByTestId("evidence-cross-check-badge");
    await expect(badge).toHaveAttribute("data-state", "matches", {
      timeout: 10_000,
    });
    await expect(badge).toContainText("Matches CI");
    await card.getByTestId("test-run-summary-toggle").click();
    const details = card.getByTestId("test-run-summary-details");
    await expect(details).toBeVisible();
    await expect(details).toContainText("Crew CI");
    await waitForAnimations(page);
    await card.screenshot({
      path: "test-results/evidence-cross-check/01-matches.png",
    });
  });

  test("Diverges shows claimed vs CI and leaves Accept/Reject usable", async ({
    page,
  }) => {
    const root = rootId("02");
    const panel = await openProjectThread(
      page,
      root,
      basePr([{ name: "Desktop Fast", state: "FAILURE" }]),
    );
    await emitEvidence(page, root, "Tests: 14 passed, 0 failed", "test-run");
    const card = panel.getByTestId("evidence-card-test-run");
    await expect(
      card.getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "diverges", { timeout: 10_000 });
    await expect(card.getByTestId("evidence-cross-check-detail")).toContainText(
      "Local 14✓ 0✗",
    );
    await expect(card.getByTestId("evidence-cross-check-detail")).toContainText(
      "Desktop Fast",
    );
    await expect(card.getByTestId("evidence-accept")).toBeVisible();
    await expect(card.getByTestId("evidence-reject")).toBeVisible();
    await card.getByTestId("evidence-accept").click();
    await expect(card.getByTestId("evidence-reaction-accepted")).toBeVisible();
    await waitForAnimations(page);
    await card.screenshot({
      path: "test-results/evidence-cross-check/02-diverges.png",
    });
  });

  test("CI running then live-recomputes to Diverges", async ({ page }) => {
    const root = rootId("03");
    const panel = await openProjectThread(
      page,
      root,
      basePr([
        { name: "Crew CI", state: "SUCCESS" },
        { name: "Desktop E2E", state: "IN_PROGRESS" },
      ]),
    );
    await emitEvidence(page, root, "Tests: 14 passed, 0 failed", "test-run");
    const card = panel.getByTestId("evidence-card-test-run");
    await expect(
      card.getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "ci-running", { timeout: 10_000 });
    await waitForAnimations(page);
    await card.screenshot({
      path: "test-results/evidence-cross-check/03-ci-running.png",
    });

    await page.evaluate(
      ({ branch, status }) => {
        window.__BUZZ_E2E_SET_THREAD_GITHUB_BY_BRANCH__?.(branch, status);
      },
      {
        branch: BRANCH,
        status: basePr([{ name: "Desktop E2E", state: "FAILURE" }]),
      },
    );
    await expect(
      card.getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "diverges", { timeout: 10_000 });
  });

  test("Not comparable for metrics kind", async ({ page }) => {
    const root = rootId("04");
    const panel = await openProjectThread(
      page,
      root,
      basePr([{ name: "Crew CI", state: "SUCCESS" }]),
    );
    await emitEvidence(
      page,
      root,
      "before: 120ms | after: 80ms | delta: -40ms",
      "metrics",
    );
    const card = panel.getByTestId("evidence-card-metrics");
    await expect(
      card.getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "not-comparable", { timeout: 10_000 });
    await waitForAnimations(page);
    await card.screenshot({
      path: "test-results/evidence-cross-check/04-not-comparable-metrics.png",
    });
  });

  test("Not comparable for unlinked channel evidence", async ({ page }) => {
    await installMockBridge(page, {
      searchProfiles: [
        {
          pubkey: AGENT,
          displayName: "Evidence Agent",
          ownerPubkey: OWNER,
          isAgent: true,
        },
      ],
    });
    await page.goto("/");
    await page.getByTestId("channel-general").click();
    await waitForLiveChannel(page);
    await page.evaluate(
      ({ pubkey }) =>
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "general",
          content: "Tests: 1 passed, 0 failed",
          pubkey,
          extraTags: [["crew-evidence", "test-run"]],
        }),
      { pubkey: AGENT },
    );
    const channelCard = page.getByTestId("evidence-card-test-run");
    await expect(
      channelCard.getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "not-comparable");
    await waitForAnimations(page);
    await channelCard.screenshot({
      path: "test-results/evidence-cross-check/05-not-comparable-channel.png",
    });
  });

  test("diff-stat Matches within tolerance", async ({ page }) => {
    const root = rootId("06");
    const panel = await openProjectThread(
      page,
      root,
      basePr([{ name: "Crew CI", state: "SUCCESS" }], {
        additions: 100,
        deletions: 20,
        changedFiles: 5,
      }),
    );
    await emitEvidence(
      page,
      root,
      "Diff: +105/−18 across 6 files",
      "diff-stat",
    );
    await expect(
      panel
        .getByTestId("evidence-card-diff-stat")
        .getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "matches", { timeout: 10_000 });
  });
});
