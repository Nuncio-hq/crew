import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const ROOT = "c".repeat(64);
const CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const BRANCH = "buzz/cccccccccccc";
const AGENT = TEST_IDENTITIES.alice.pubkey;
const OWNER = "deadbeef".repeat(8);

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

async function openProjectThread(page: Page, github = basePr([])) {
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
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );

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
    { agent: AGENT, eventId: ROOT },
  );

  const row = page
    .getByTestId("message-row")
    .filter({ hasText: "cross-check the evidence badge" });
  await expect(row).toBeVisible();
  await row.hover();
  await row.getByRole("button", { name: "Reply" }).click();
  const panel = page.getByTestId("message-thread-panel");
  await expect(panel).toBeVisible();

  await page.evaluate(
    ({ agentPubkey, branchName, channelId, root }) => {
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
            conversationId: "conversation-evidence-cross-check",
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
            conversationId: "conversation-evidence-cross-check",
            sessionId: null,
            turnId: `turn-${root.slice(0, 8)}`,
            payload: {
              rootEventId: root,
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
      root: ROOT,
    },
  );

  await expect(panel.getByText("Ready", { exact: true })).toBeVisible();
  return panel;
}

async function emitEvidence(
  page: Page,
  content: string,
  kind: "test-run" | "diff-stat" | "metrics" | "before-after-visual",
) {
  return page.evaluate(
    ({ body, evidenceKind, pubkey, root }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName: "general",
        content: body,
        pubkey,
        parentEventId: root,
        extraTags: [["crew-evidence", evidenceKind]],
      }),
    { body: content, evidenceKind: kind, pubkey: AGENT, root: ROOT },
  );
}

test.describe("evidence–CI cross-check badge (#175)", () => {
  test("Matches CI for green test-run claim", async ({ page }) => {
    const panel = await openProjectThread(
      page,
      basePr([{ name: "Crew CI", state: "SUCCESS" }]),
    );
    await emitEvidence(page, "Tests: 14 passed, 0 failed", "test-run");
    const card = panel.getByTestId("evidence-card-test-run");
    await expect(card).toBeVisible();
    const badge = card.getByTestId("evidence-cross-check-badge");
    await expect(badge).toHaveAttribute("data-state", "matches");
    await expect(badge).toContainText("Matches CI");
    await waitForAnimations(page);
    await card.screenshot({
      path: "test-results/evidence-cross-check/01-matches.png",
    });
  });

  test("Diverges shows claimed vs CI and leaves Accept/Reject usable", async ({
    page,
  }) => {
    const panel = await openProjectThread(
      page,
      basePr([{ name: "Desktop Fast", state: "FAILURE" }]),
    );
    await emitEvidence(page, "Tests: 14 passed, 0 failed", "test-run");
    const card = panel.getByTestId("evidence-card-test-run");
    await expect(
      card.getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "diverges");
    await expect(card.getByTestId("evidence-cross-check-detail")).toContainText(
      "Claimed: 14 passed, 0 failed",
    );
    await expect(card.getByTestId("evidence-cross-check-detail")).toContainText(
      "Desktop Fast — FAILURE",
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
    const panel = await openProjectThread(
      page,
      basePr([
        { name: "Crew CI", state: "SUCCESS" },
        { name: "Desktop E2E", state: "IN_PROGRESS" },
      ]),
    );
    await emitEvidence(page, "Tests: 14 passed, 0 failed", "test-run");
    const card = panel.getByTestId("evidence-card-test-run");
    await expect(
      card.getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "ci-running");
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

  test("Not comparable for metrics and unlinked channel evidence", async ({
    page,
  }) => {
    const panel = await openProjectThread(
      page,
      basePr([{ name: "Crew CI", state: "SUCCESS" }]),
    );
    await emitEvidence(
      page,
      "before: 120ms | after: 80ms | delta: -40ms",
      "metrics",
    );
    await expect(
      panel
        .getByTestId("evidence-card-metrics")
        .getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "not-comparable");
    await waitForAnimations(page);
    await panel.getByTestId("evidence-card-metrics").screenshot({
      path: "test-results/evidence-cross-check/04-not-comparable-metrics.png",
    });

    await page.getByRole("button", { name: "Close panel" }).click();
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
    await page.waitForFunction(
      () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
    );
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
    const panel = await openProjectThread(
      page,
      basePr([{ name: "Crew CI", state: "SUCCESS" }], {
        additions: 100,
        deletions: 20,
        changedFiles: 5,
      }),
    );
    await emitEvidence(page, "Diff: +105/−18 across 6 files", "diff-stat");
    await expect(
      panel
        .getByTestId("evidence-card-diff-stat")
        .getByTestId("evidence-cross-check-badge"),
    ).toHaveAttribute("data-state", "matches");
  });
});
