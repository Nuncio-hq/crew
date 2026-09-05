import type { Locator } from "@playwright/test";
import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { expandProjectPlumbing } from "../helpers/projectPlumbing";

const SHOTS = "test-results/project-pr-review";
const RECOVERY_SHOTS = "test-results/project-pr-conflict-recovery";
const REVIEWER_AGENT_PUBKEY = "a".repeat(64);
const DEFAULT_MOCK_PUBKEY = "deadbeef".repeat(8);

// The projects surface is a preview feature — opt in before the app mounts.
// Must run before installMockBridge so React reads the override on mount.
async function enableProjectsFeature(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "buzz-feature-overrides-v1",
      JSON.stringify({ projects: true }),
    );
  });
}

async function openBuzzProject(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByTestId("projects-section-projects").click();
  const projectEntry = page
    .locator(
      '[data-testid="project-card-buzz"], [data-testid="project-row-buzz"]',
    )
    .first();
  await expect(projectEntry).toBeVisible({ timeout: 10_000 });
  await projectEntry.click();
  await expandProjectPlumbing(page);
}

async function expectLocalRepositoryOpenAction(
  page: import("@playwright/test").Page,
) {
  const openButton = page.getByRole("button", { name: "Open", exact: true });
  await expect(openButton).toHaveAttribute(
    "title",
    "Open local repository folder",
  );
  await expect(
    page.getByRole("link", { name: "Open", exact: true }),
  ).toHaveCount(0);
}

test("same-second request changes supersedes approval", async ({ page }) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    Date.now = () => 1_900_000_000_000;
  });
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  const aliceRow = page
    .getByTestId("project-pull-request-row")
    .filter({ has: page.getByRole("button", { name: "alice", exact: true }) })
    .first();
  await expect(aliceRow).toBeVisible({ timeout: 10_000 });
  await aliceRow.getByRole("button", { name: /^#/ }).click();

  await page.getByRole("button", { name: "Approve", exact: true }).click();
  const approveDialog = page.getByRole("dialog", {
    name: "Approve review",
  });
  await approveDialog
    .getByRole("textbox", { name: "Approval summary" })
    .fill("Approved at the fixed second.");
  await approveDialog
    .getByRole("button", { name: "Approve", exact: true })
    .click();
  await expect(page.getByText("Review approved.")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toHaveCount(0);

  const commentComposer = page.getByTestId(
    "project-pull-request-comment-composer",
  );
  await commentComposer
    .getByRole("button", { name: "Comment", exact: true })
    .click();
  await page.getByRole("menuitemradio", { name: "Request changes" }).click();
  await commentComposer
    .locator('[contenteditable="true"]')
    .fill("Changes requested at the same fixed second.");
  await commentComposer.getByRole("button", { name: "Send message" }).click();
  await expect(page.getByText("Changes requested.")).toBeVisible();

  const [approvalEvent, changeRequestEvent] = await page.evaluate(() => {
    const decisions =
      window.__BUZZ_E2E_SIGNED_EVENTS__?.filter(
        (event) =>
          event.kind === 1 &&
          event.tags.some(
            (tag) =>
              tag[0] === "t" &&
              (tag[1] === "approval" || tag[1] === "changes-requested"),
          ),
      ) ?? [];
    return [decisions.at(-2), decisions.at(-1)];
  });
  expect(approvalEvent?.tags).toContainEqual(["t", "approval"]);
  expect(changeRequestEvent?.tags).toContainEqual(["t", "changes-requested"]);
  expect(changeRequestEvent?.createdAt).toBeGreaterThan(
    approvalEvent?.createdAt ?? 0,
  );
});

test("PR creator/owner can toggle draft, request reviews, and approve", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_EVENT_KINDS__ = [1631];
  });
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  const prRows = page.getByTestId("project-pull-request-row");
  await expect(prRows.first()).toBeVisible({ timeout: 10_000 });

  // Pick a PR authored by alice: the viewer is not the author, so the
  // Approve button must be available alongside the owner status controls.
  const aliceRow = prRows
    .filter({ has: page.getByRole("button", { name: "alice", exact: true }) })
    .first();
  await expect(aliceRow).toBeVisible();
  await aliceRow.getByRole("button", { name: /^#/ }).click();

  const header = page.getByRole("heading", { level: 3 });
  await expect(header.first()).toBeVisible();
  const sourceChannelLink = page.getByRole("button", {
    name: "Open author-claimed origin channel #general",
    exact: true,
  });
  await expect(sourceChannelLink).toBeVisible();

  // Owner viewing an open PR: draft toggle and both review decisions are offered.
  const morePullRequestActions = page.getByRole("button", {
    name: "More review actions",
  });
  const approve = page.getByRole("button", { name: "Approve", exact: true });
  const commentComposer = page.getByTestId(
    "project-pull-request-comment-composer",
  );
  const reviewMode = commentComposer.getByRole("button", {
    name: "Comment",
    exact: true,
  });
  await expect(morePullRequestActions).toBeVisible();
  await expect(approve).toBeVisible();
  await expect(reviewMode).toBeVisible();

  // Request a review from bob via the centered reviewer dialog.
  await page
    .getByTestId("project-reviewers-content")
    .getByRole("button", { name: "Add Reviewer", exact: true })
    .click();
  await expect(
    page.getByRole("dialog").getByRole("heading", { name: "Add reviewer" }),
  ).toBeVisible();
  await waitForAnimations(page);
  await page.screenshot({
    fullPage: false,
    path: `${SHOTS}/00-add-reviewer-dialog.png`,
  });
  await page.getByTestId("project-reviewer-search").fill("bob");
  await page
    .getByTestId(`project-reviewer-result-${TEST_IDENTITIES.bob.pubkey}`)
    .evaluate((button) => {
      button.click();
      button.click();
    });
  await expect(page.getByText("Review requested.")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_SIGNED_EVENTS__?.filter(
            (event) =>
              event.kind === 1 &&
              event.tags.some(
                (tag) => tag[0] === "t" && tag[1] === "review-request",
              ),
          ).length ?? 0,
      ),
    )
    .toBe(1);
  const reviewHistoryToggle = page.getByTestId(
    "project-pull-request-review-history-toggle",
  );
  await expect(reviewHistoryToggle).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.getByTestId("project-pull-request-timeline-row"),
  ).toHaveCount(1);
  // The requested reviewer appears in the reviewers row and default timeline.
  await expect(page.getByText("Requested a review from bob")).toBeVisible({
    timeout: 10_000,
  });

  await waitForAnimations(page);
  await page.screenshot({
    fullPage: false,
    path: `${SHOTS}/01-review-requested.png`,
  });

  await reviewMode.click();
  await page.getByRole("menuitemradio", { name: "Request changes" }).click();
  await commentComposer
    .locator('[contenteditable="true"]')
    .fill("Please handle the empty state before merging.");
  await commentComposer.getByRole("button", { name: "Send message" }).click();
  await expect(page.getByText("Changes requested.")).toBeVisible();
  await expect(reviewMode).toHaveText("Comment");
  await expect(
    page.getByText("Please handle the empty state before merging."),
  ).toBeVisible();
  await expect(
    page
      .getByTestId("project-pull-request-timeline-row")
      .filter({ hasText: "requested changes" }),
  ).toBeVisible({ timeout: 10_000 });
  await expect(
    page.getByText("Changes requested", { exact: true }),
  ).toHaveCount(0);
  const changeRequestEvent = await page.evaluate(() =>
    window.__BUZZ_E2E_SIGNED_EVENTS__
      ?.filter(
        (event) =>
          event.kind === 1 &&
          event.tags.some(
            (tag) => tag[0] === "t" && tag[1] === "changes-requested",
          ),
      )
      .at(-1),
  );
  expect(changeRequestEvent?.content).toBe(
    "Please handle the empty state before merging.",
  );
  expect(changeRequestEvent?.tags).toContainEqual(["c", expect.any(String)]);
  const reviewDecisionEvents = await page.evaluate(
    () =>
      window.__BUZZ_E2E_SIGNED_EVENTS__?.filter(
        (event) =>
          event.kind === 1 &&
          event.tags.some(
            (tag) =>
              tag[0] === "t" &&
              (tag[1] === "approval" || tag[1] === "changes-requested"),
          ),
      ) ?? [],
  );
  expect(reviewDecisionEvents).toHaveLength(1);

  await waitForAnimations(page);
  await page.screenshot({
    fullPage: false,
    path: `${SHOTS}/05-changes-requested.png`,
  });
  const changeRequestRow = page
    .getByTestId("project-pull-request-timeline-row")
    .filter({ hasText: "requested changes" })
    .first();
  await changeRequestRow.getByRole("button").first().hover();
  await expect(page.getByTestId("user-profile-popover")).toBeVisible();
  await page.mouse.move(0, 0);
  await expect(page.getByTestId("user-profile-popover")).toBeHidden();

  await expect(reviewHistoryToggle).toContainText("Collapse review history");
  const expandedReviewRows = page.getByTestId(
    "project-pull-request-timeline-row",
  );
  await expect(expandedReviewRows).toHaveCount(2);
  await expect(expandedReviewRows.nth(0)).toContainText(
    "Requested a review from bob",
  );
  await expect(expandedReviewRows.nth(1)).toContainText("requested changes");
  await expect(approve).toBeVisible();
  await reviewHistoryToggle.click();
  await expect(reviewHistoryToggle).toContainText("Show 2 earlier activities");
  await expect(
    page.getByTestId("project-pull-request-timeline-row"),
  ).toHaveCount(0);
  await expect(changeRequestRow).toBeHidden();

  // Replace the completed change request with an approval. Both decisions
  // remain tied to the current commit and their timestamps preserve order.
  await approve.click();
  const approveDialog = page.getByRole("dialog", {
    name: "Approve review",
  });
  await approveDialog
    .getByRole("textbox", { name: "Approval summary" })
    .fill("Ready to merge.");
  await approveDialog
    .getByRole("button", { name: "Approve", exact: true })
    .click();
  await expect(page.getByText("Review approved.")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Approve", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("Approved", { exact: true })).toHaveCount(0);
  const approvalEvent = await page.evaluate(() =>
    window.__BUZZ_E2E_SIGNED_EVENTS__
      ?.filter(
        (event) =>
          event.kind === 1 &&
          event.tags.some((tag) => tag[0] === "t" && tag[1] === "approval"),
      )
      .at(-1),
  );
  expect(approvalEvent?.content).toBe("Ready to merge.");
  expect(approvalEvent?.tags).toContainEqual(["c", expect.any(String)]);
  expect(approvalEvent?.createdAt).toBeGreaterThan(
    changeRequestEvent?.createdAt ?? 0,
  );
  await reviewHistoryToggle.click();
  await expect(page.getByText("Ready to merge.")).toBeVisible({
    timeout: 10_000,
  });
  await expect(
    page.getByTestId("project-pull-request-timeline-row"),
  ).toHaveCount(3);

  await waitForAnimations(page);
  await page.screenshot({
    fullPage: false,
    path: `${SHOTS}/02-approved.png`,
  });

  // Histories over three entries show only the latest three until explicitly
  // expanded. Collapsing the whole timeline preserves that inner choice.
  await commentComposer
    .locator('[contenteditable="true"]')
    .fill("Remember the expanded history state.");
  await commentComposer.getByRole("button", { name: "Send message" }).click();
  await expect(page.getByText("Comment posted.")).toBeVisible();
  const timelineRows = page.getByTestId("project-pull-request-timeline-row");
  const earlierActivities = page.getByTestId(
    "project-pull-request-earlier-activities",
  );
  await expect(timelineRows).toHaveCount(3);
  await expect(earlierActivities).toContainText("Show 1 earlier activity");

  await reviewHistoryToggle.click();
  await expect(timelineRows).toHaveCount(0);
  await reviewHistoryToggle.click();
  await expect(timelineRows).toHaveCount(3);
  await expect(earlierActivities).toBeVisible();

  await earlierActivities.click();
  await expect(timelineRows).toHaveCount(4);
  await reviewHistoryToggle.click();
  await expect(timelineRows).toHaveCount(0);
  await reviewHistoryToggle.click();
  await expect(timelineRows).toHaveCount(4);
  await expect(earlierActivities).toHaveCount(0);

  // Convert to draft: badge flips to Draft and the ready button appears.
  await morePullRequestActions.click();
  await page.getByRole("menuitem", { name: "Convert to draft" }).click();
  await expect(page.getByText("Converted to draft.")).toBeVisible();
  const readyForReview = page.getByRole("button", {
    name: "Ready for review",
  });
  await expect(readyForReview).toBeVisible({ timeout: 10_000 });
  await expect(morePullRequestActions).toBeVisible();

  await waitForAnimations(page);
  await page.screenshot({
    fullPage: false,
    path: `${SHOTS}/03-draft.png`,
  });

  // And back: Ready for review restores the Open state.
  await readyForReview.click();
  await expect(page.getByText("Marked as ready for review.")).toBeVisible();
  await expect(morePullRequestActions).toBeVisible({ timeout: 10_000 });

  // Closing is reversible, unlike merging: a closed PR can be reopened.
  await morePullRequestActions.click();
  const closePullRequest = page.getByRole("menuitem", {
    name: "Close review",
  });
  await closePullRequest.click();
  await expect(page.getByText("Review closed.")).toBeVisible();
  const reopenPullRequest = page.getByRole("button", {
    name: "Reopen review",
  });
  await expect(reopenPullRequest).toBeVisible({ timeout: 10_000 });
  await expect(closePullRequest).toHaveCount(0);

  await reopenPullRequest.click();
  await expect(page.getByText("Review reopened.")).toBeVisible();
  await expect(morePullRequestActions).toBeVisible({ timeout: 10_000 });

  await page.getByRole("button", { name: "Merge", exact: true }).click();
  await expect(page.getByTestId("merge-pull-request-confirm")).toBeVisible();
  await page.getByTestId("merge-pull-request-confirm-button").click();
  await expect(page.getByText("Merged feature into main.")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_SIGNED_EVENTS__?.filter(
            (event) => event.kind === 1631,
          ).length ?? 0,
      ),
    )
    .toBe(1);
  await expect(
    page.getByRole("button", {
      name: "Publish merged status",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", {
      name: "Publish merged status",
      exact: true,
    })
    .click();
  await expect(page.getByText("Published merged review status.")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_SIGNED_EVENTS__?.filter(
            (event) => event.kind === 1631,
          ).length ?? 0,
      ),
    )
    .toBe(1);
  const mergedEvent = await page.evaluate(() =>
    window.__BUZZ_E2E_SIGNED_EVENTS__
      ?.filter((event) => event.kind === 1631)
      .at(-1),
  );
  expect(mergedEvent?.tags).toContainEqual([
    "merge-commit",
    "abcdef0123456789abcdef0123456789abcdef01",
  ]);
  expect(mergedEvent?.tags.some((tag) => tag[0] === "e")).toBe(true);
  const mergeCommandCount = await page.evaluate(
    () =>
      window.__BUZZ_E2E_COMMANDS__?.filter(
        (command) => command === "merge_project_pull_request",
      ).length ?? 0,
  );
  expect(mergeCommandCount).toBe(1);
  const mergePayload = await page.evaluate(() =>
    window.__BUZZ_E2E_COMMAND_PAYLOADS__?.find(
      (entry) => entry.command === "merge_project_pull_request",
    ),
  );
  expect(mergePayload?.payload).toMatchObject({
    input: {
      expectedCommit: expect.any(String),
      sourceBranch: expect.any(String),
      targetBranch: "main",
      targetOwner: DEFAULT_MOCK_PUBKEY,
    },
  });

  await sourceChannelLink.click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
});

test("merge conflicts offer persistent terminal recovery", async ({ page }) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);
  await page.evaluate(() => {
    window.__BUZZ_E2E_PROJECT_MERGE_ERROR__ = {
      code: "merge_conflict",
      message: "Pull request has merge conflicts.",
      recovery: {
        action: "open_terminal",
        sourceBranch: "feature",
        targetBranch: "main",
      },
    };
  });

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  const aliceRow = page
    .getByTestId("project-pull-request-row")
    .filter({ has: page.getByRole("button", { name: "alice", exact: true }) })
    .first();
  await aliceRow.getByRole("button", { name: /^#/ }).click();
  await page.getByRole("button", { name: "Merge", exact: true }).click();
  await page.getByTestId("merge-pull-request-confirm-button").click();

  const recovery = page.getByTestId("merge-conflict-recovery");
  await expect(recovery).toBeVisible();
  await expect(
    recovery.getByRole("button", { name: "Copy commands" }),
  ).toBeDisabled();
  await waitForAnimations(page);
  await recovery.screenshot({
    path: `${RECOVERY_SHOTS}/01-merge-conflict.png`,
  });
  await recovery.getByRole("button", { name: "Resolve in Terminal" }).click();
  await expect(
    page.getByText("Recovery commit fetched and terminal opened."),
  ).toBeVisible();
  await expect(
    page.getByText("Recovery commit fetched and terminal opened."),
  ).toBeHidden({ timeout: 10_000 });
  await expect(recovery).toContainText("git switch 'main'");
  await expect(recovery).toContainText("git merge 'refs/buzz/merge-recovery/");
  await expect(
    recovery.getByRole("button", { name: "Copy commands" }),
  ).toBeEnabled();
  await waitForAnimations(page);
  await recovery.screenshot({
    path: `${RECOVERY_SHOTS}/02-merge-conflict-prepared.png`,
  });

  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_COMMAND_PAYLOADS__?.find(
            (entry) => entry.command === "open_project_merge_recovery_terminal",
          ) ?? null,
      ),
    )
    .toMatchObject({
      command: "open_project_merge_recovery_terminal",
      payload: {
        input: {
          expectedCommit: expect.any(String),
          sourceBranch: "feature",
          targetBranch: "main",
        },
      },
    });
});

test("reviewer can leave a commit-scoped inline diff comment", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  const aliceRow = page
    .getByTestId("project-pull-request-row")
    .filter({ has: page.getByRole("button", { name: "alice", exact: true }) })
    .first();
  await aliceRow.getByRole("button", { name: /^#/ }).click();
  await page
    .getByTestId("project-detail-section-files")
    .getByRole("button", { name: /^Files changed/ })
    .click();

  const diffLine = page
    .getByTestId("project-diff-line")
    .filter({ hasText: "function CommunityTabs({ selectedCommitHash })" });
  await expect(diffLine).toBeVisible({ timeout: 10_000 });
  await diffLine.hover();
  await diffLine.getByTestId("project-diff-add-comment").click();

  const composer = page.getByTestId("project-inline-comment-thread");
  await composer
    .locator("[contenteditable='true']")
    .fill("Please add a type for this parameter.");
  await composer.getByRole("button", { name: "Comment", exact: true }).click();
  await page.getByRole("menuitemradio", { name: "Request changes" }).click();
  await composer.getByRole("button", { name: "Send message" }).click();
  await expect(page.getByText("Changes requested.")).toBeVisible();

  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__BUZZ_E2E_SIGNED_EVENTS__?.find(
          (event) => event.content === "Please add a type for this parameter.",
        ),
      ),
    )
    .not.toBeUndefined();
  const inlineCommentEvent = await page.evaluate(() =>
    window.__BUZZ_E2E_SIGNED_EVENTS__?.find(
      (event) => event.content === "Please add a type for this parameter.",
    ),
  );
  expect(inlineCommentEvent?.tags).toContainEqual(["t", "inline-comment"]);
  expect(inlineCommentEvent?.tags).toContainEqual(["t", "changes-requested"]);
  expect(inlineCommentEvent?.tags).toContainEqual(["c", expect.any(String)]);
  expect(inlineCommentEvent?.tags).toContainEqual([
    "file",
    "desktop/src/features/projects/ui/ProjectDetailScreen.tsx",
  ]);
  expect(inlineCommentEvent?.tags).toContainEqual(["side", "new"]);
  expect(inlineCommentEvent?.tags).toContainEqual(["line", "3"]);
  await expect(page.getByTestId("project-inline-comment")).toContainText(
    "Please add a type for this parameter.",
  );

  // Collapsing the diff returns focus to the review activity below it.
  await page
    .getByTestId("project-detail-section-files")
    .getByRole("button", { name: /^Files changed/ })
    .click();
  await expect(
    page.getByTestId("project-pull-request-review-history-toggle"),
  ).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.getByText("Please add a type for this parameter."),
  ).toBeVisible();
  await expect(
    page.getByText("desktop/src/features/projects/ui/ProjectDetailScreen.tsx"),
  ).toBeVisible();
  await waitForAnimations(page);
  await page.screenshot({
    fullPage: false,
    path: `${SHOTS}/04-inline-comment.png`,
  });

  await page
    .getByRole("button", {
      name: "Open desktop/src/features/projects/ui/ProjectDetailScreen.tsx new line 3 in Files changed",
    })
    .click();
  await expect(
    page.getByTestId("project-detail-section-files"),
  ).toHaveAttribute("data-open", "true");
  const focusedLine = page.getByTestId("project-diff-focused-line");
  await expect(focusedLine).toBeVisible();
  await expect(focusedLine).toHaveAttribute(
    "data-path",
    "desktop/src/features/projects/ui/ProjectDetailScreen.tsx",
  );
  await expect(focusedLine).toHaveAttribute("data-side", "new");
  await expect(focusedLine).toHaveAttribute("data-line", "3");
  await focusedLine.click();
  await expect(page.getByTestId("project-diff-focused-line")).toHaveCount(0);
});

test("managed agent repository owner can merge", async ({ page }) => {
  await enableProjectsFeature(page);
  await page.addInitScript((owner) => {
    window.__BUZZ_E2E_PROJECT_OWNER_OVERRIDE__ = owner;
  }, TEST_IDENTITIES.alice.pubkey);
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: TEST_IDENTITIES.alice.pubkey,
        name: "Brain",
      },
      {
        pubkey: REVIEWER_AGENT_PUBKEY,
        name: "Reviewer Bot",
      },
    ],
  });
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  const agentRow = page
    .getByTestId("project-pull-request-row")
    .filter({ has: page.getByRole("button", { name: "Brain", exact: true }) })
    .first();
  await expect(agentRow).toBeVisible({ timeout: 10_000 });
  await agentRow.getByRole("button", { name: /^#/ }).click();
  await page
    .getByTestId("project-reviewers-content")
    .getByRole("button", { name: "Add Reviewer", exact: true })
    .click();
  await page.getByTestId("project-reviewer-search").fill("Reviewer Bot");
  await page
    .getByTestId(`project-reviewer-result-${REVIEWER_AGENT_PUBKEY}`)
    .click();
  await expect(page.getByText("Review requested.")).toBeVisible();
  const reviewRequestPayload = await page.evaluate(() =>
    window.__BUZZ_E2E_COMMAND_PAYLOADS__?.find(
      (entry) => entry.command === "sign_project_pull_request_review_request",
    ),
  );
  expect(reviewRequestPayload?.payload).toMatchObject({
    input: {
      reviewers: [REVIEWER_AGENT_PUBKEY],
      targetOwner: TEST_IDENTITIES.alice.pubkey,
    },
  });
  await page.getByRole("button", { name: "More review actions" }).click();
  const closePullRequest = page.getByRole("menuitem", {
    name: "Close review",
  });
  await expect(closePullRequest).toBeVisible();
  await closePullRequest.click();
  await expect(page.getByText("Review closed.")).toBeVisible();
  await page.getByRole("button", { name: "Reopen review" }).click();
  await expect(page.getByText("Review reopened.")).toBeVisible();
  const statusPayloads = await page.evaluate(() =>
    window.__BUZZ_E2E_COMMAND_PAYLOADS__?.filter(
      (entry) => entry.command === "sign_project_pull_request_status",
    ),
  );
  expect(statusPayloads).toHaveLength(2);
  expect(statusPayloads?.map((entry) => entry.payload)).toEqual([
    expect.objectContaining({
      input: expect.objectContaining({
        status: "closed",
        targetOwner: TEST_IDENTITIES.alice.pubkey,
      }),
    }),
    expect.objectContaining({
      input: expect.objectContaining({
        status: "open",
        targetOwner: TEST_IDENTITIES.alice.pubkey,
      }),
    }),
  ]);
  await page.getByRole("button", { name: "Merge", exact: true }).click();
  await page.getByTestId("merge-pull-request-confirm-button").click();
  await expect(page.getByText("Merged feature into main.")).toBeVisible();

  const mergePayload = await page.evaluate(() =>
    window.__BUZZ_E2E_COMMAND_PAYLOADS__?.find(
      (entry) => entry.command === "merge_project_pull_request",
    ),
  );
  expect(mergePayload?.payload).toMatchObject({
    input: {
      expectedCommit: expect.any(String),
      sourceBranch: expect.any(String),
      targetBranch: "main",
      targetOwner: TEST_IDENTITIES.alice.pubkey,
    },
  });
});

test("viewer without repository ownership cannot merge", async ({ page }) => {
  await enableProjectsFeature(page);
  await page.addInitScript((owner) => {
    window.__BUZZ_E2E_PROJECT_OWNER_OVERRIDE__ = owner;
  }, TEST_IDENTITIES.alice.pubkey);
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: REVIEWER_AGENT_PUBKEY,
        name: "Reviewer Bot",
      },
    ],
  });
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  const prRow = page.getByTestId("project-pull-request-row").first();
  await expect(prRow).toBeVisible({ timeout: 10_000 });
  await prRow.getByRole("button", { name: /^#/ }).click();

  await expect(
    page.getByRole("button", { name: "Merge", exact: true }),
  ).toHaveCount(0);
  const mergeCommandCount = await page.evaluate(
    () =>
      window.__BUZZ_E2E_COMMANDS__?.filter(
        (command) => command === "merge_project_pull_request",
      ).length ?? 0,
  );
  expect(mergeCommandCount).toBe(0);

  const authorizationError = await page.evaluate(async (targetOwner) => {
    try {
      await window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__?.(
        "merge_project_pull_request",
        {
          input: {
            expectedCommit: "1".repeat(40),
            pullRequestAuthor: "2".repeat(64),
            pullRequestId: "3".repeat(64),
            repoAddress: `30617:${targetOwner}:buzz`,
            sourceBranch: "feature/untrusted",
            statusCreatedAt: 1,
            targetBranch: "main",
            targetOwner,
          },
        },
      );
      return null;
    } catch (error) {
      return error instanceof Error ? error.message : String(error);
    }
  }, TEST_IDENTITIES.alice.pubkey);
  expect(authorizationError).toContain(
    "Only the repository owner or the owner of its managed agent",
  );
});

test("project pull requests preserve partial results from batched queries", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [1619];
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByRole("button", { name: "Reviews", exact: true }).click();

  await expect(
    page.getByRole("button", { name: /^View / }).first(),
  ).toBeVisible();
  await expect(
    page.getByText(/Some review details could not be loaded/),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();

  const workItemFilters = await page.evaluate(
    () =>
      window.__BUZZ_E2E_PROJECT_QUERY_FILTERS__?.filter(
        (filter) => filter.limit === 2_000,
      ) ?? [],
  );
  expect(
    workItemFilters
      .map((filter) => JSON.stringify([...(filter.kinds ?? [])].sort()))
      .sort(),
  ).toEqual(
    [[1], [1618, 1621], [1619], [1630, 1631, 1632, 1633]]
      .map((kinds) => JSON.stringify(kinds))
      .sort(),
  );
  expect(
    workItemFilters.every((filter) => (filter["#a"]?.length ?? 0) > 1),
  ).toBe(true);
  const expectedRepoAddresses = [
    `30617:${DEFAULT_MOCK_PUBKEY}:buzz`,
    `30617:${TEST_IDENTITIES.alice.pubkey}:relay-tools`,
    `30617:${TEST_IDENTITIES.bob.pubkey}:design-system`,
  ].sort();
  for (const filter of workItemFilters) {
    expect([...(filter["#a"] ?? [])].sort()).toEqual(expectedRepoAddresses);
  }

  await page.evaluate(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [];
  });
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(
    page.getByText(/Some review details could not be loaded/),
  ).toHaveCount(0);
});

test("project pull request author rollover stays identity-only", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByRole("button", { name: "Reviews", exact: true }).click();
  await page.getByRole("button", { name: "List layout" }).click();

  const row = page.locator('[data-testid^="projects-pr-row-"]').first();
  const author = row.getByTestId("projects-pr-author");
  await expect(author).toBeVisible();
  await expect(
    author.locator(
      '[data-testid="projects-pr-author-avatar-image"], [data-testid="projects-pr-author-avatar-fallback"]',
    ),
  ).toBeVisible();

  const authorLabel = (
    await author.getByTestId("projects-pr-author-label").innerText()
  ).trim();
  await author.hover();
  const rollover = page.getByTestId("projects-pr-author-rollover");
  await expect(rollover).toBeVisible();
  await expect(rollover).toContainText(authorLabel);
  await expect(rollover).toContainText(/Agent|Person/);
  await expect(rollover).not.toContainText("Created");
  await expect(page.getByTestId("user-profile-popover")).toHaveCount(0);
});

test("project issue author rollover matches pull requests", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await page.getByRole("button", { name: "List layout" }).click();

  const row = page.locator('[data-testid^="projects-issue-row-"]').first();
  const author = row.getByTestId("projects-issue-author");
  await expect(author).toBeVisible();
  await expect(
    author.locator(
      '[data-testid="projects-issue-author-avatar-image"], [data-testid="projects-issue-author-avatar-fallback"]',
    ),
  ).toBeVisible();

  const authorLabel = (
    await author.getByTestId("projects-issue-author-label").innerText()
  ).trim();
  await author.hover();
  const rollover = page.getByTestId("projects-issue-author-rollover");
  await expect(rollover).toBeVisible();
  await expect(rollover).toContainText(authorLabel);
  await expect(rollover).toContainText(/Agent|Person/);
  await expect(rollover).not.toContainText("Created");
  await expect(page.getByTestId("user-profile-popover")).toHaveCount(0);
});

test("project pull requests report aggregate root query failures", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [1618];
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByRole("button", { name: "Reviews", exact: true }).click();

  await expect(page.getByText("Could not load reviews")).toBeVisible();
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();
  await expect(page.getByText("No reviews yet")).toHaveCount(0);

  await page.evaluate(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [];
  });
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.getByText("Could not load reviews")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: /^View / }).first(),
  ).toBeVisible();
});

test("project issues preserve partial results from aggregate queries", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [1];
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByRole("button", { name: "Tasks", exact: true }).click();

  await expect(
    page.getByRole("button", { name: /^View / }).first(),
  ).toBeVisible();
  await expect(
    page.getByText("Some task details could not be loaded."),
  ).toBeVisible();
  await expect(page.getByText(/Missing comments\./)).toBeVisible();
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();

  await page.evaluate(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [];
  });
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(
    page.getByText("Some task details could not be loaded."),
  ).toHaveCount(0);
});

test("project overview reports aggregate work-item failures", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [1618];
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  await expect(
    page.getByText("Could not load project activity."),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Retry" })).toBeVisible();

  await page.evaluate(() => {
    window.__BUZZ_E2E_REJECT_PROJECT_QUERY_KINDS__ = [];
  });
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.getByText("Could not load project activity.")).toHaveCount(
    0,
  );
});

test("project overview does not paint a background behind its cards", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  const landing = page.getByTestId("projects-outcome-landing");
  await expect(landing).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  const outcomeCards = landing.locator(
    '[data-testid^="project-outcome-card-"]',
  );
  await expect(outcomeCards.first()).toBeVisible();
  const outcomeCardCount = await outcomeCards.count();
  for (let index = 0; index < outcomeCardCount; index += 1) {
    await expect(outcomeCards.nth(index)).toHaveCSS(
      "background-color",
      "rgba(0, 0, 0, 0)",
    );
    await expect(outcomeCards.nth(index)).toHaveCSS("border-style", "solid");
  }
});

test("project without a checkout can clone from repository actions", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);

  await expect(
    page.getByRole("button", { name: "Remote", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Remote", exact: true }),
  ).toHaveClass(/\bborder-input\/40\b/);
  await expect(page.getByRole("button", { name: /main/ })).toHaveClass(
    /\bborder-input\/40\b/,
  );
  await expect(
    page.getByRole("button", { name: "Clone", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Fetch", exact: true }).click();
  await expect(page.getByText("Remote state refreshed.")).toBeVisible();

  await page.getByRole("button", { name: "Clone", exact: true }).click();
  await expect(page.getByText("Cloned repository.")).toBeVisible();
  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).toContain("clone_project_repository");
});

test("overview scope controls align with list actions and selectable groups", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/#/projects");
  for (const section of [
    "projects",
    "repositories",
    "issues",
    "prs",
  ] as const) {
    await page.getByTestId(`projects-section-${section}`).click();
    const header = page.getByTestId("projects-list-header");
    await header.getByRole("button", { name: "List layout" }).click();
    const scope = header.getByRole("button", { name: /^Filter / });
    const layout = header.getByRole("button", { name: "List layout" });
    const scopeBox = await requiredBox(scope);
    const layoutBox = await requiredBox(layout);
    expect(scopeBox).not.toBeNull();
    expect(layoutBox).not.toBeNull();
    expect(
      Math.abs(
        scopeBox.y + scopeBox.height / 2 - (layoutBox.y + layoutBox.height / 2),
      ),
    ).toBeLessThanOrEqual(2);
    expect(scopeBox.x + scopeBox.width).toBeLessThan(layoutBox.x);
    const rowSelector = {
      projects: '[data-testid^="project-row-"]',
      repositories: '[data-testid^="repository-row-"]',
      issues: '[data-testid^="projects-issue-row-"]',
      prs: '[data-testid^="projects-pr-row-"]',
    }[section];
    const row = page.locator(rowSelector).first();
    await expect(row).toBeVisible();
    const rowBox = await requiredBox(row);
    const headerBox = await requiredBox(header);
    expect(Math.abs(rowBox.x - headerBox.x)).toBeLessThanOrEqual(2);
    expect(Math.abs(rowBox.width - headerBox.width)).toBeLessThanOrEqual(2);
  }
  await page.getByTestId("projects-section-issues").click();
  const group = page.getByTestId("projects-issue-project-group-header").first();
  await page.mouse.move(1, 1);
  const icon = group.getByTestId("project-group-icon");
  const checkbox = group.getByTestId("projects-group-select");
  await expect(icon).toHaveCSS("opacity", "1");
  await group.hover();
  await expect(icon).toHaveCSS("opacity", "0");
  const iconBox = await requiredBox(
    group.getByTestId("project-group-leading-icon"),
  );
  const checkboxBox = await requiredBox(checkbox);
  expect(
    Math.abs(
      iconBox.x + iconBox.width / 2 - (checkboxBox.x + checkboxBox.width / 2),
    ),
  ).toBeLessThanOrEqual(1);
  await checkbox.click();
  await expect(page.getByTestId("projects-selection-summary")).toContainText(
    /tasks?/,
  );
  await expect(checkbox).toBeChecked();
});

test("project outcomes remain the default and channel groups preserve navigation", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/#/projects");
  const landing = page.getByTestId("projects-outcome-landing");
  await expect(
    landing.getByRole("heading", { name: "What needs attention?" }),
  ).toBeVisible();
  const cards = landing.locator('[data-testid^="project-outcome-card-"]');
  await expect(cards.first()).toBeVisible();
  await expect(
    landing.getByRole("button", { name: "Open project buzz", exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("projects-section-all")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.getByTestId("projects-section-channels").click();
  const groups = page.getByTestId("projects-channel-project-group");
  const rows = page.getByTestId("project-channel-row");
  await expect(groups.first()).toBeVisible();
  await expect(rows.first()).toBeVisible();
  expect(await groups.count()).toBeLessThanOrEqual(await rows.count());
  await expect(page.getByTestId("projects-channels-list")).toContainText(
    "buzz",
  );
  await expect(page.getByTestId("projects-channels-list")).toContainText(
    "relay-tools",
  );
  await expect(page.getByTestId("projects-channels-list")).toContainText(
    "design-system",
  );
  await page.getByTestId("projects-section-all").click();
  await landing
    .getByRole("button", { name: "Open project buzz", exact: true })
    .click();
  await expect(page.getByTestId("project-repository-picker")).toContainText(
    "buzz",
  );
  await expandProjectPlumbing(page);
  await expect(
    page.getByRole("tab", { name: "Review", exact: true }),
  ).toBeVisible();
});

test("selected review discussion preserves the channel draft and deduplicates links", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page, {
    projectAccessChannelId: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
  });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  const composer = page.getByTestId("message-input");
  await composer.fill("Existing review notes.");
  for (let round = 0; round < 2; round += 1) {
    await page.goto("/#/projects");
    await page.getByTestId("projects-section-prs").click();
    await page.getByRole("button", { name: "List layout" }).click();
    const row = page.locator('[data-testid^="projects-pr-row-"]').first();
    await row.getByTestId("projects-row-select").click();
    await page.getByTestId("projects-selection-chat-agent").click();
    await expect(page.getByTestId("chat-title")).toHaveText("general");
    await expect(composer).toContainText("Existing review notes.");
    await expect(composer).toContainText("Let's talk about this review:");
    const draft = await composer.innerText();
    expect(draft.match(/Let's talk about this review:/g)).toHaveLength(1);
    expect(await composer.locator("[data-composer-buzz-link]").count()).toBe(1);
  }
  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).not.toContain("send_channel_message");
});

test("overview selection actions clear without changing the list width", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/#/projects");
  await page.getByTestId("projects-section-issues").click();
  await page.getByRole("button", { name: "List layout" }).click();
  const rows = page.locator('[data-testid^="projects-issue-row-"]');
  const width = (await requiredBox(rows.first())).width;
  await rows.first().getByTestId("projects-row-select").click();
  const actions = page.getByTestId("projects-selection-actions");
  await expect(actions).toBeVisible();
  await expect(actions.getByTestId("projects-selection-copy")).toBeVisible();
  await actions.getByTestId("projects-selection-discuss").click();
  await expect(
    page.getByTestId("projects-selection-channel-choices"),
  ).toBeVisible();
  await expect
    .poll(async () => (await requiredBox(rows.first())).width)
    .toBe(width);
  await waitForAnimations(page);
  await actions
    .locator("..")
    .screenshot({ path: `${SHOTS}/05-overview-selection-actions.png` });
  await page.getByTestId("projects-selection-clear").click();
  await expect(actions).toHaveCount(0);
  await expect(rows.getByRole("checkbox", { checked: true })).toHaveCount(0);
  expect((await requiredBox(rows.first())).width).toBe(width);
});

test("Projects search replaces section tabs and filters every overview section", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/#/projects");
  for (const [section, selector, needle] of [
    ["all", '[data-testid^="project-outcome-card-"]', "buzz"],
    ["projects", '[data-testid^="project-row-"]', "buzz"],
    ["repositories", '[data-testid^="repository-row-"]', "relay-tools"],
    ["issues", '[data-testid^="projects-issue-row-"]', "onboarding"],
    ["prs", '[data-testid^="projects-pr-row-"]', "filter"],
    ["channels", '[data-testid="project-channel-row"]', "buzz"],
  ] as const) {
    await page.getByTestId(`projects-section-${section}`).click();
    if (section !== "all" && section !== "channels")
      await page.getByRole("button", { name: "List layout" }).click();
    const rows = page.locator(selector);
    await expect(rows.first()).toBeVisible();
    const count = await rows.count();
    await page.getByTestId("projects-activity-search").click();
    const input = page.getByTestId("projects-section-search-input");
    await expect(input).toBeFocused();
    await expect(page.getByTestId(`projects-section-${section}`)).toHaveCount(
      0,
    );
    if (section !== "all" && section !== "channels") {
      await expect(
        page.getByTestId("projects-page-tabs").getByRole("combobox"),
      ).toBeVisible();
    }
    await input.fill("no-such-project-result-38219");
    await expect(rows).toHaveCount(0);
    await input.fill(needle);
    await expect(rows.first()).toBeVisible();
    expect(await rows.count()).toBeLessThanOrEqual(count);
    if (section === "repositories") {
      await expect(
        page.getByTestId("repository-row-relay-tools"),
      ).toBeVisible();
      await expect(page.getByTestId("repository-row-buzz")).toHaveCount(0);
      await waitForAnimations(page);
      await page
        .locator("[data-buzz-content-surface]")
        .screenshot({ path: `${SHOTS}/06-overview-filtered-search.png` });
    }
    await input.press("Escape");
    await expect(page.getByTestId("projects-section-search")).toHaveCount(0);
    await expect(page.getByTestId(`projects-section-${section}`)).toBeVisible();
    await expect(rows).toHaveCount(count);
  }
  await page.getByTestId("projects-activity-search").click();
  await page.getByTestId("projects-section-search-close").click();
  await expect(page.getByTestId("projects-section-search")).toHaveCount(0);
});

test("overview list selection supports keyboard, ranges and channel choices", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/#/projects");
  for (const [section, prefix, noun] of [
    ["projects", "project-row-", "project"],
    ["repositories", "repository-row-", "repository"],
    ["issues", "projects-issue-row-", "task"],
    ["prs", "projects-pr-row-", "review"],
  ] as const) {
    await page.getByTestId(`projects-section-${section}`).click();
    await page.getByRole("button", { name: "List layout" }).click();
    const rows = page.locator(`[data-testid^="${prefix}"]`);
    const first = rows.first().getByTestId("projects-row-select");
    await rows.first().getByRole("button").first().focus();
    await page.keyboard.press("Tab");
    await expect(first).toBeFocused();
    await page.keyboard.press("Space");
    await expect(first).toBeChecked();
    await expect(page.getByTestId("projects-selection-summary")).toContainText(
      `1 ${noun}`,
    );
    const count = Math.min(3, await rows.count());
    expect(count).toBeGreaterThan(1);
    await rows
      .nth(count - 1)
      .getByTestId("projects-row-select")
      .click({ modifiers: ["Shift"] });
    await expect(rows.getByRole("checkbox", { checked: true })).toHaveCount(
      count,
    );
    await expect(page.getByTestId("projects-selection-summary")).toContainText(
      String(count),
    );
    await page.getByTestId("projects-selection-discuss").click();
    await expect(
      page.getByTestId("projects-selection-related-channel").first(),
    ).toBeVisible();
    await page.getByTestId("projects-selection-search-channels").click();
    await expect(page.getByTestId("channel-browser-search")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("projects-selection-summary")).toBeVisible();
    await page.getByTestId("projects-selection-clear").click();
    await expect(rows.getByRole("checkbox", { checked: true })).toHaveCount(0);
  }
});

test("overview selection resets when its section or search scope changes", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/#/projects");
  await page.getByTestId("projects-section-issues").click();
  await page.getByRole("button", { name: "List layout" }).click();
  const rows = page.locator('[data-testid^="projects-issue-row-"]');
  await rows.first().getByTestId("projects-row-select").click();
  await expect(page.getByTestId("projects-selection-summary")).toBeVisible();
  await page.getByTestId("projects-section-prs").click();
  await expect(page.getByTestId("projects-selection-summary")).toHaveCount(0);
  await page.getByTestId("projects-section-issues").click();
  await expect(rows.getByRole("checkbox", { checked: true })).toHaveCount(0);
  await rows.first().getByTestId("projects-row-select").click();
  await page.getByTestId("projects-activity-search").click();
  await page
    .getByTestId("projects-section-search-input")
    .fill("no-such-selected-task-9281");
  await expect(rows).toHaveCount(0);
  await expect(page.getByTestId("projects-selection-summary")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(rows.first()).toBeVisible();
  await expect(rows.getByRole("checkbox", { checked: true })).toHaveCount(0);
});

test("repository changes discard selection before preparing channel discussion", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);
  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  const row = page.getByTestId("project-issue-row").first();
  const selectedTitle = (
    await row.locator('[data-projects-text-priority="primary"]').innerText()
  ).trim();
  await row.getByTestId("projects-row-select").click();
  await expect(page.getByTestId("projects-selection-summary")).toContainText(
    "1 task",
  );
  await page.getByTestId("project-repository-picker").click();
  await page.getByTestId("project-repository-relay-tools").click();
  await expect(page.getByTestId("project-repository-picker")).toContainText(
    "relay-tools",
  );
  await expect(page.getByTestId("projects-selection-summary")).toHaveCount(0);
  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  await page
    .getByTestId("project-issue-row")
    .first()
    .getByTestId("projects-row-select")
    .click();
  await page.getByTestId("projects-selection-chat-agent").click();
  const draft = page.getByTestId("message-input");
  await expect(draft).toContainText("Let's talk about this task:");
  await expect(draft).not.toContainText(selectedTitle);
  await expect(draft.locator("[data-composer-buzz-link]")).toHaveAttribute(
    "data-href",
    /relay-tools/,
  );
  expect(
    await page.evaluate(() => window.__BUZZ_E2E_COMMANDS__ ?? []),
  ).not.toContain("send_channel_message");
});

test("overview lists position identifying and generic icons consistently", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  await page.getByTestId("projects-section-projects").click();
  await page.getByRole("button", { name: "List layout" }).click();
  const projectRow = page.locator('[data-testid^="project-row-"]').first();
  const projectTitle = projectRow.getByTestId("project-entity-title");
  const projectDescription = projectRow.getByTestId("projects-row-description");
  const projectRepositoryCount = projectRow.getByTestId("projects-row-context");
  await expect(projectDescription).toBeVisible();
  await expect(projectRepositoryCount.locator("svg")).toBeVisible();
  await expect(projectRepositoryCount).not.toContainText(/repositor/i);
  await expect(projectRepositoryCount).toHaveAttribute(
    "title",
    /^\d+ repositor(?:y|ies)$/,
  );
  const [projectTitleBox, projectDescriptionBox] = await Promise.all([
    projectTitle.boundingBox(),
    projectDescription.boundingBox(),
  ]);
  expect(projectTitleBox).not.toBeNull();
  expect(projectDescriptionBox).not.toBeNull();
  expect(projectDescriptionBox?.x ?? 0).toBeGreaterThanOrEqual(
    (projectTitleBox?.x ?? 0) + (projectTitleBox?.width ?? 0),
  );
  await expect(projectDescription).toHaveCSS(
    "font-size",
    await projectTitle.evaluate(
      (element) => getComputedStyle(element).fontSize,
    ),
  );

  for (const section of ["repositories", "issues", "prs"] as const) {
    await page.getByTestId(`projects-section-${section}`).click();
    await page.getByRole("button", { name: "List layout" }).click();
    const rows = page.locator(
      section === "repositories"
        ? '[data-testid^="repository-row-"]'
        : section === "issues"
          ? '[data-testid^="projects-issue-row-"]'
          : '[data-testid^="projects-pr-row-"]',
    );
    const row = rows.first();
    await expect(row).toBeVisible();
    await expect(row.getByTestId("project-entity-description")).toHaveCount(0);
    if (section === "repositories") {
      const activityBar = row.getByTestId("repositories-row-activity-bar");
      const date = row.getByTestId("repositories-row-date");
      await expect(activityBar).toBeVisible();
      const [barBox, dateBox] = await Promise.all([
        activityBar.boundingBox(),
        date.boundingBox(),
      ]);
      expect(barBox).not.toBeNull();
      expect(dateBox).not.toBeNull();
      expect(barBox?.width ?? 0).toBe(176);
      expect((barBox?.x ?? 0) + (barBox?.width ?? 0)).toBeLessThanOrEqual(
        dateBox?.x ?? 0,
      );
      const segment = activityBar
        .getByTestId("project-activity-segment")
        .first();
      const segmentLabel = await segment.getAttribute("aria-label");
      await segment.hover();
      await expect(page.getByRole("tooltip")).toContainText(segmentLabel ?? "");
    }
    const title = row.getByTestId("project-entity-title");
    const icon = row.getByTestId(
      section === "repositories"
        ? "project-entity-leading-icon"
        : "project-entity-title-icon",
    );
    const [titleBox, iconBox] = await Promise.all([
      title.boundingBox(),
      icon.boundingBox(),
    ]);
    expect(titleBox).not.toBeNull();
    expect(iconBox).not.toBeNull();
    if (section === "repositories") {
      expect((iconBox?.x ?? 0) + (iconBox?.width ?? 0)).toBeLessThanOrEqual(
        titleBox?.x ?? 0,
      );
    } else {
      expect(iconBox?.x ?? 0).toBeGreaterThanOrEqual(
        (titleBox?.x ?? 0) + (titleBox?.width ?? 0),
      );
    }
    const iconColumns = await rows
      .getByTestId(
        section === "repositories"
          ? "project-entity-leading-icon"
          : "project-entity-title-icon",
      )
      .evaluateAll((icons) =>
        icons.slice(0, 5).map((icon) => icon.getBoundingClientRect().x),
      );
    expect(
      Math.max(...iconColumns) - Math.min(...iconColumns),
    ).toBeLessThanOrEqual(1);
    await expect(title).toHaveCSS("text-overflow", "ellipsis");
  }
});

test("repository context control collapses its panel without covering the workspace", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);
  const info = page.getByTestId("project-right-panel-repository-tab");
  const panel = page.getByTestId("project-repository-actions-panel");
  const workspace = page.getByTestId("project-detail-scroll");
  await expect(info).toHaveAttribute("aria-pressed", "true");
  await expect(panel).toBeVisible();
  const panelBox = await requiredBox(panel);
  const workspaceBox = await requiredBox(workspace);
  expect(workspaceBox.x + workspaceBox.width).toBeLessThanOrEqual(
    panelBox.x + 1,
  );
  await page.getByTestId("project-repository-branch-trigger").click();
  await expect(
    page.getByRole("menuitemradio", { name: "main", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await info.click();
  await expect(info).toHaveAttribute("aria-pressed", "false");
  await expect(panel).toHaveCount(0);
  await expect
    .poll(async () => (await requiredBox(workspace)).width)
    .toBeGreaterThan(workspaceBox.width);
  await info.click();
  await expect(panel).toBeVisible();
  expect((await requiredBox(panel)).width).toBe(panelBox.width);
  expect((await requiredBox(workspace)).width).toBe(workspaceBox.width);
});

test("selecting repository workspace rows switches the context pod to the cluster", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByTestId("projects-section-projects").click();
  await page
    .locator(
      '[data-testid="project-card-buzz"], [data-testid="project-row-buzz"]',
    )
    .first()
    .click();
  await expandProjectPlumbing(page);
  await expect(page.getByTestId("project-repository-picker")).toContainText(
    "buzz",
  );

  await page.getByRole("tab", { name: "Commits" }).click();
  const commitRows = page.getByTestId("project-activity-feed-item");
  await expect(commitRows.first()).toBeVisible({ timeout: 10_000 });
  await expectSinglePrimaryTextColumn(commitRows.first());
  await commitRows.first().getByTestId("projects-row-select").click();
  await page.mouse.move(1, 1);
  await expect(
    commitRows
      .first()
      .getByTestId("projects-row-select")
      .locator("..")
      .locator(".."),
  ).toHaveCSS("opacity", "1");
  if ((await commitRows.count()) > 1) {
    await expect(
      commitRows
        .nth(1)
        .getByTestId("projects-row-select")
        .locator("..")
        .locator(".."),
    ).toHaveCSS("opacity", "0");
  }
  await expect(page.getByTestId("projects-overview-context-title")).toHaveText(
    "1 commit",
  );
  await expect(
    page.getByTestId("projects-selection-create-review"),
  ).toBeVisible();
  await page.getByTestId("projects-selection-clear").click();
  await expect(page.getByTestId("projects-overview-context-title")).toHaveCount(
    0,
  );

  await page.getByRole("tab", { name: "Tasks" }).click();
  const issueRows = page.getByTestId("project-issue-row");
  await expect(issueRows.first()).toBeVisible();
  await expectSinglePrimaryTextColumn(issueRows.first());
  const taskGroup = page.getByTestId("project-work-item-group").first();
  const taskGroupRows = taskGroup.getByTestId("project-issue-row");
  const taskGroupRowCount = await taskGroupRows.count();
  const taskGroupHeader = taskGroup.getByTestId(
    "project-work-item-group-header",
  );
  await taskGroupHeader.getByRole("button").click();
  await expect(taskGroupRows).toHaveCount(0);
  await taskGroupHeader.getByRole("button").click();
  await expect(taskGroupRows).toHaveCount(taskGroupRowCount);
  await taskGroupHeader.hover();
  await taskGroup.getByTestId("projects-group-select").click();
  await expect(page.getByTestId("projects-overview-context-title")).toHaveText(
    `${taskGroupRowCount} ${taskGroupRowCount === 1 ? "task" : "tasks"}`,
  );
  await page.getByTestId("projects-selection-clear").click();
  await issueRows.first().getByTestId("projects-row-select").click();
  await page.mouse.move(1, 1);
  await expect(
    issueRows
      .first()
      .getByTestId("projects-row-select")
      .locator("..")
      .locator(".."),
  ).toHaveCSS("opacity", "1");
  if ((await issueRows.count()) > 1) {
    await expect(
      issueRows
        .nth(1)
        .getByTestId("projects-row-select")
        .locator("..")
        .locator(".."),
    ).toHaveCSS("opacity", "0");
  }
  await expect(page.getByTestId("projects-overview-context-title")).toHaveText(
    "1 task",
  );
  await page.keyboard.press("Escape");

  await page.getByRole("tab", { name: "Review" }).click();
  const reviewRows = page.getByTestId("project-pull-request-row");
  await expect(reviewRows.first()).toBeVisible();
  await expectSinglePrimaryTextColumn(reviewRows.first());
  const reviewRowOrder = await reviewRows.first().evaluate((row) => {
    const left = (selector: string) =>
      row.querySelector<HTMLElement>(selector)?.getBoundingClientRect().left ??
      Number.NaN;
    return {
      checkbox: left('[data-testid="projects-row-select"]'),
      identifier: left('[data-testid="project-work-item-identifier"]'),
      status: left('[data-testid="project-work-item-status-icon"]'),
      title: left('[data-projects-text-priority="primary"]'),
    };
  });
  expect(
    Math.abs(reviewRowOrder.checkbox - reviewRowOrder.status),
  ).toBeLessThanOrEqual(1);
  expect(reviewRowOrder.status).toBeLessThan(reviewRowOrder.title);
  expect(reviewRowOrder.title).toBeLessThan(reviewRowOrder.identifier);
  await reviewRows.first().getByTestId("projects-row-select").click();
  await expect(
    reviewRows.first().getByTestId("project-work-item-status-icon"),
  ).toHaveCSS("opacity", "0");
  await expect(
    reviewRows
      .first()
      .getByTestId("projects-row-select")
      .locator("..")
      .locator(".."),
  ).toHaveCSS("opacity", "1");
  await expect(page.getByTestId("projects-overview-context-title")).toHaveText(
    "1 review",
  );
});

test("repository context resize tracks the pointer and resets its width", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);
  const panel = page.getByTestId("project-repository-actions-panel");
  const handle = panel.getByTestId("right-auxiliary-pane-resize-handle");
  await expect(panel).toBeVisible();
  const initial = await requiredBox(panel);
  const box = await requiredBox(handle);
  const startX = box.x + box.width / 2;
  const startY = box.y + box.height / 2;
  await handle.dispatchEvent("pointerdown", {
    button: 0,
    buttons: 1,
    clientX: startX,
    clientY: startY,
    pointerId: 1,
    pointerType: "mouse",
  });
  await page.mouse.move(startX - 40, startY);
  await expect
    .poll(async () => Math.round((await requiredBox(panel)).width))
    .toBe(Math.round(initial.width) + 40);
  await expect(panel).toHaveCSS("transition-duration", "0s");
  await page.mouse.up();
  await expect(handle).toHaveAttribute(
    "title",
    "Drag to resize. Double-click to reset width.",
  );
  await handle.dblclick();
  await expect
    .poll(async () => Math.round((await requiredBox(panel)).width))
    .toBe(Math.round(initial.width));
});

test("repository rows identify their git host", async ({ page }) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await page.getByRole("button", { name: "Repositories", exact: true }).click();
  await page.getByRole("button", { name: "List layout" }).click();

  const buzzHostIcon = page
    .getByTestId("repository-row-buzz")
    .getByTestId("repository-host-icon");
  await expect(buzzHostIcon).toHaveAttribute(
    "aria-label",
    "Buzz-hosted repository",
  );
  await expect(
    page
      .getByTestId("repository-row-relay-tools")
      .getByTestId("repository-host-icon"),
  ).toHaveAttribute("aria-label", "Git data hosted on github.com");
});

test("project subsections do not paint backgrounds behind list or grid items", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  for (const section of ["Repositories", "Reviews", "Tasks"] as const) {
    await page.getByRole("button", { name: section, exact: true }).click();
    await page.getByRole("button", { name: "List layout" }).click();

    const listItems = page.locator(
      section === "Repositories"
        ? '[data-testid^="repository-row-"]'
        : section === "Reviews"
          ? '[data-testid^="projects-pr-row-"]'
          : '[data-testid^="projects-issue-row-"]',
    );
    await expect(listItems.first()).toBeVisible();
    const listItemCount = await listItems.count();
    for (let index = 0; index < listItemCount; index += 1) {
      await expect(listItems.nth(index)).toHaveCSS(
        "background-color",
        "rgba(0, 0, 0, 0)",
      );
    }

    await page.getByRole("button", { name: "Grid layout" }).click();
    const gridCards = page.locator(
      section === "Repositories"
        ? '[data-testid^="repository-card-"]'
        : "[data-projects-grid-card]",
    );
    await expect(gridCards.first()).toBeVisible();
    const gridCardCount = await gridCards.count();
    for (let index = 0; index < gridCardCount; index += 1) {
      await expect(gridCards.nth(index)).toHaveCSS(
        "background-color",
        "rgba(0, 0, 0, 0)",
      );
      await expect(gridCards.nth(index)).toHaveCSS("border-style", "solid");
    }
  }
});

test("all project grid cards cap body copy at two lines", async ({ page }) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  for (const section of [
    "projects",
    "repositories",
    "issues",
    "prs",
  ] as const) {
    await page.getByTestId(`projects-section-${section}`).click();
    await page.getByRole("button", { name: "Grid layout" }).click();
    if (section === "projects") {
      const projectNames = page.getByTestId("project-grid-card-name");
      await expect(projectNames.first()).toBeVisible();
      for (const name of await projectNames.all()) {
        await expect(name).toHaveCSS("white-space", "normal");
        await expect(name).toHaveCSS("overflow", "visible");
      }
    }
    if (section === "issues" || section === "prs") {
      const cards = page.locator("[data-projects-grid-card]");
      const titles = page.getByTestId("projects-grid-card-title");
      const indicators = page.getByTestId("projects-grid-card-indicator");
      const cardCount = await cards.count();
      await expect(titles).toHaveCount(cardCount);
      await expect(indicators).toHaveCount(cardCount);
      for (const title of await titles.all()) {
        await expect(title).toHaveCSS("white-space", "nowrap");
        await expect(title).toHaveCSS("overflow", "hidden");
        await expect(title).toHaveCSS("text-overflow", "ellipsis");
      }
      for (const card of await cards.all()) {
        await expect(card.getByRole("button")).toHaveCount(1);
      }
    }
    const bodies = page.getByTestId("projects-grid-card-body");
    await expect(bodies.first()).toBeVisible();
    const measurements = await bodies.evaluateAll((elements) =>
      elements.map((element) => {
        const style = getComputedStyle(element);
        return {
          height: element.getBoundingClientRect().height,
          lineClamp: style.webkitLineClamp,
          lineHeight: Number.parseFloat(style.lineHeight),
        };
      }),
    );
    for (const measurement of measurements) {
      expect(measurement.lineClamp).toBe("2");
      expect(measurement.height).toBeLessThanOrEqual(
        measurement.lineHeight * 2 + 1,
      );
    }
  }
});

test("project detail content areas do not paint background fills", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);

  const expectVisiblePanelsToBeTransparent = async ({
    bordered = true,
    required = false,
  }: {
    bordered?: boolean;
    required?: boolean;
  } = {}) => {
    const panels = page.locator("[data-project-detail-panel]:visible");
    const panelCount = await panels.count();
    if (required) await expect(panels.first()).toBeVisible();
    for (let index = 0; index < panelCount; index += 1) {
      await expect(panels.nth(index)).toHaveCSS(
        "background-color",
        "rgba(0, 0, 0, 0)",
      );
      if (bordered) {
        await expect(panels.nth(index)).toHaveCSS("border-style", "solid");
      } else {
        await expect(panels.nth(index)).toHaveCSS("border-width", "0px");
      }
    }
  };

  for (const tab of [
    "Overview",
    "Files",
    "Commits",
    "Tasks",
    "Review",
    "Contributors",
  ]) {
    await page.getByRole("tab", { name: tab, exact: true }).click();
    await expectVisiblePanelsToBeTransparent();
  }

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  const pullRequest = page.getByTestId("project-pull-request-row").first();
  await expect(pullRequest).toBeVisible();
  await pullRequest.getByRole("button", { name: /^#/ }).click();
  await expectVisiblePanelsToBeTransparent({
    bordered: false,
    required: true,
  });
});

test("project without a checkout offers fetch feedback and cloning", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);

  await expect(
    page.getByRole("button", { name: "Remote", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Remote", exact: true }),
  ).toHaveClass(/\bborder-input\/40\b/);
  await expect(page.getByRole("button", { name: /main/ })).toHaveClass(
    /\bborder-input\/40\b/,
  );
  await expect(
    page.getByRole("button", { name: "Clone", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Fetch", exact: true }).click();
  await expect(page.getByText("Remote state refreshed.")).toBeVisible();

  await page.getByRole("button", { name: "Remote", exact: true }).click();
  const cloneItem = page.getByRole("menuitem", {
    name: "Local missing Clone",
  });
  await expect(cloneItem.getByText("Local missing")).toHaveClass(
    /text-muted-foreground/,
  );
  await expect(cloneItem.getByText("Clone", { exact: true })).toHaveClass(
    /\bborder-input\/60\b/,
  );
  await cloneItem.click();
  await expect(page.getByText("Cloned repository.")).toBeVisible();
  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).toContain("clone_project_repository");
  const openFolder = page.getByRole("button", { name: "Open", exact: true });
  await expect(openFolder).toHaveAttribute(
    "title",
    "Open local repository folder",
  );
  await openFolder.click();
  expect(
    await page.evaluate(() => window.__BUZZ_E2E_COMMANDS__ ?? []),
  ).toContain("open_project_repository_folder");
  await expect(page.getByText("Couldn’t open repository folder")).toHaveCount(
    0,
  );
});

test("project branches can be created from the selected remote branch", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page, {
    projectHeadBranch: "master",
    relaySelf: TEST_IDENTITIES.bob.pubkey,
  });
  await openBuzzProject(page);

  await page.getByRole("button", { name: /main/ }).click();
  await page.getByTestId("project-create-branch").click();
  await page
    .getByTestId("project-create-branch-name")
    .fill("feature/branch-management");
  await page.getByTestId("project-create-branch-submit").click();

  await expect(
    page.getByText("Created branch feature/branch-management from main.", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: /feature\/branch-management/ }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: /feature\/branch-management/ })
    .click();
  await expect(
    page.getByRole("menuitemradio", { name: "feature/branch-management" }),
  ).toBeVisible();
  await page.getByRole("menuitemradio", { name: "main" }).click();
  await page.getByRole("button", { name: /main/ }).click();
  await expect(
    page.getByRole("menuitemradio", { name: "feature/branch-management" }),
  ).toBeVisible();
  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).toContain("create_project_remote_branch");

  await openBuzzProject(page);
  await page.getByRole("button", { name: /main/ }).click();
  await expect(
    page.getByRole("menuitemradio", { name: "feature/branch-management" }),
  ).toBeVisible();
});

test("repository tags can be browsed as immutable remote snapshots", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("button", { name: /main/ }).click();
  await expect(page.getByText("Tags", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("menuitemradio", { name: /v1\.0\.0.*0123456/ }),
  ).toBeVisible();
  await page.getByRole("menuitemradio", { name: /v1\.0\.0.*0123456/ }).click();

  await expect(page.getByRole("button", { name: /v1\.0\.0/ })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Remote", exact: true }),
  ).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => {
        const call = [...(window.__BUZZ_E2E_COMMAND_PAYLOADS__ ?? [])]
          .reverse()
          .find((entry) => entry.command === "get_project_repo_snapshot");
        return (call?.payload as { targetRef?: string } | undefined)?.targetRef;
      }),
    )
    .toBe("refs/tags/v1.0.0");
  await page.getByRole("button", { name: /v1\.0\.0/ }).click();
  await expect(page.getByTestId("project-create-branch")).toHaveCount(0);
  await expect(page.getByTestId("project-delete-branch")).toHaveCount(0);

  await page.getByRole("menuitemradio", { name: "main" }).click();
  await page.getByRole("button", { name: /main/ }).click();
  await expect(page.getByTestId("project-create-branch")).toBeVisible();
});

test("project branches can be deleted but the default branch cannot", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("button", { name: /main/ }).click();
  await expect(page.getByTestId("project-delete-branch")).toBeDisabled();
  await page.getByTestId("project-create-branch").click();
  await page
    .getByTestId("project-create-branch-name")
    .fill("feature/delete-me");
  await page.getByTestId("project-create-branch-submit").click();
  await expect(
    page.getByRole("button", { name: /feature\/delete-me/ }),
  ).toBeVisible();
  await page.getByRole("button", { name: /feature\/delete-me/ }).click();
  await page.getByTestId("project-delete-branch").click();
  await expect(page.getByTestId("project-delete-branch-dialog")).toBeVisible();
  await page.getByTestId("project-delete-branch-submit").click();

  await expect(
    page.getByText("Deleted branch feature/delete-me.", { exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: /main/ })).toBeVisible();
  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).toContain("delete_project_remote_branch");
});

test("external repositories stay on local source after a branch round trip", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    const commit = "0123456789abcdef0123456789abcdef01234567";
    const localBranch =
      "wintermute/entity-link-recipient-cards-with-a-long-branch-name";
    window.sessionStorage.setItem(
      "buzz-e2e-project-branches",
      JSON.stringify({ "relay-tools": { [localBranch]: commit } }),
    );
    window.__BUZZ_E2E_PROJECT_REPO_SYNC_STATUS__ = {
      local_path: "/tmp/buzz/REPOS/relay-tools",
      local_branch: localBranch,
      local_branches: ["main", localBranch],
      local_head: commit,
      local_short_head: commit.slice(0, 7),
      remote_branch: localBranch,
      remote_head: commit,
      remote_short_head: commit.slice(0, 7),
      merge_base: commit,
      ahead_count: 0,
      behind_count: 0,
      has_uncommitted_changes: false,
      has_untracked_files: false,
      can_push: false,
      push_block_reason: "Local branch is already pushed.",
      can_pull: false,
      pull_block_reason: "Local branch is up to date.",
    };
    window.__BUZZ_E2E_PROJECT_LOCAL_REPO_SNAPSHOT__ = {
      path: "/tmp/buzz/REPOS/relay-tools",
      snapshot: {
        latest_commit: null,
        commits: [],
        contributors: [],
        files: [
          {
            path: "README.md",
            kind: "text",
            size: 21,
            preview_content: "# Local branch README",
            last_changed_at: null,
            latest_commit: null,
          },
        ],
      },
    };
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await openBuzzProject(page);
  await page.getByTestId("project-repository-picker").click();
  await page.getByTestId("project-repository-relay-tools").click();
  // An existing checkout is selected automatically for an external host.
  await expect(page.getByRole("button", { name: /^Local / })).toBeVisible();

  await expect(
    page.getByRole("heading", { name: "Local branch README" }),
  ).toBeVisible();
  await expectLocalRepositoryOpenAction(page);

  await page.getByRole("button", { name: /main/ }).click();
  await page
    .getByRole("menuitemradio", {
      name: "wintermute/entity-link-recipient-cards-with-a-long-branch-name",
    })
    .click();
  const branchTrigger = page.getByTestId("project-repository-branch-trigger");
  await expect(
    page.getByRole("button", {
      name: /wintermute\/entity-link-recipient-cards-with-a-long-branch-name/,
    }),
  ).toBeVisible();
  await expect
    .poll(() =>
      branchTrigger.evaluate(
        (element) => element.scrollWidth <= element.clientWidth,
      ),
    )
    .toBe(true);
  await expect
    .poll(() =>
      branchTrigger
        .locator("span")
        .evaluate((element) => element.scrollWidth > element.clientWidth),
    )
    .toBe(true);
  await expectLocalRepositoryOpenAction(page);

  await branchTrigger.click();
  await page.getByRole("menuitemradio", { name: "main" }).click();

  await expect(page.getByRole("button", { name: /main/ })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Local branch README" }),
  ).toBeVisible();
  await expectLocalRepositoryOpenAction(page);
  await expect(page.getByText("Code hosted on github.com")).toHaveCount(0);
});

test("repository files beyond the eager preview limit load on demand", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    const deferredFiles = [
      ...Array.from({ length: 250 }, (_, index) => ({
        path: `.agents/generated-${String(index).padStart(3, "0")}.txt`,
        kind: "blob",
        size: 7,
        preview_content: "preview",
        last_changed_at: null,
        latest_commit: null,
      })),
      {
        path: "README.md",
        kind: "blob",
        size: 17,
        preview_content: null,
        last_changed_at: null,
        latest_commit: null,
      },
      {
        path: "src/application.rs",
        kind: "blob",
        size: 16,
        preview_content: null,
        last_changed_at: null,
        latest_commit: null,
      },
    ];
    window.__BUZZ_E2E_PROJECT_LOCAL_REPO_SNAPSHOT__ = {
      path: "/tmp/buzz/REPOS/relay-tools",
      snapshot: {
        latest_commit: null,
        commits: [],
        contributors: [],
        files: deferredFiles,
      },
    };
    window.__BUZZ_E2E_PROJECT_REPO_FILE_CONTENTS__ = {
      "README.md": "# Deferred README",
      "src/application.rs": "fn deferred() {}",
    };
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await openBuzzProject(page);
  await page.getByTestId("project-repository-picker").click();
  await page.getByTestId("project-repository-relay-tools").click();
  // An existing checkout is selected automatically for an external host.
  await expect(page.getByRole("button", { name: /^Local / })).toBeVisible();

  await expect(
    page.getByRole("heading", { name: "Deferred README" }),
  ).toBeVisible();

  await page.getByRole("tab", { name: "Files" }).click();
  await page.getByRole("row", { name: "Open directory src" }).click();
  await page.getByRole("row", { name: "Open file application.rs" }).click();
  await expect(
    page.getByText("fn deferred() {}", { exact: true }),
  ).toBeVisible();

  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).toContain("get_project_local_repo_file_content");
});

test("pushed local branch can open a pull request", async ({ page }) => {
  await enableProjectsFeature(page);
  await page.addInitScript(() => {
    const commit = "1234567890abcdef1234567890abcdef12345678";
    window.__BUZZ_E2E_PROJECT_REPO_SYNC_STATUS__ = {
      local_path: "/tmp/buzz/REPOS/buzz",
      local_branch: "feature/projects-workflow",
      local_branches: ["feature/projects-workflow", "space"],
      local_head: commit,
      local_short_head: commit.slice(0, 7),
      remote_branch: "feature/projects-workflow",
      remote_head: commit,
      remote_short_head: commit.slice(0, 7),
      merge_base: "0123456789abcdef0123456789abcdef01234567",
      ahead_count: 0,
      behind_count: 0,
      has_uncommitted_changes: false,
      has_untracked_files: false,
      can_push: false,
      push_block_reason: "Local branch is already pushed.",
      can_pull: false,
      pull_block_reason: "Local branch is up to date.",
    };
  });
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("button", { name: /main/ }).click();
  await expect(
    page.getByRole("menuitemradio", { name: "space" }),
  ).toBeVisible();
  await page
    .getByRole("menuitemradio", { name: "feature/projects-workflow" })
    .click();
  await page.getByRole("tab", { name: "Review", exact: true }).click();
  await page
    .getByTestId("project-section-header")
    .getByRole("button", { name: "Create review", exact: true })
    .click();
  await expect(page.getByTestId("create-pull-request-repository")).toHaveValue(
    /:buzz$/,
  );
  await expect(page.getByTestId("create-pull-request-base-branch")).toHaveValue(
    "main",
  );
  await expect(
    page.getByTestId("create-pull-request-compare-branch"),
  ).toHaveValue("feature/projects-workflow");
  await page
    .getByTestId("create-pull-request-title")
    .fill("Complete the Projects git workflow");
  await page
    .getByTestId("create-pull-request-body")
    .fill("Adds the missing desktop write path.");
  await page.getByTestId("create-pull-request-submit").evaluate((button) => {
    button.click();
    button.click();
  });
  await expect(page.getByText("Review created.")).toBeVisible();

  const createdEvents = await page.evaluate(
    () =>
      window.__BUZZ_E2E_SIGNED_EVENTS__?.filter(
        (event) => event.kind === 1618,
      ) ?? [],
  );
  expect(createdEvents).toHaveLength(1);
  const [createdEvent] = createdEvents;
  expect(createdEvent?.tags).toContainEqual([
    "branch-name",
    "feature/projects-workflow",
  ]);
  expect(createdEvent?.tags).toContainEqual(["target-branch", "main"]);
  expect(createdEvent?.tags).toContainEqual([
    "subject",
    "Complete the Projects git workflow",
  ]);
});

test("project issue can be created from the issues header", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  await page
    .getByTestId("project-section-header")
    .getByRole("button", { name: "Create task", exact: true })
    .click();
  await page
    .getByTestId("create-issue-title")
    .fill("Document the broken workflow");
  await page
    .getByTestId("create-issue-body")
    .fill("The project workflow needs a clear repair path.");
  await page.getByTestId("create-issue-submit").click();
  await expect(page.getByText("Task created.")).toBeVisible();

  const createdEvent = await page.evaluate(() =>
    window.__BUZZ_E2E_SIGNED_EVENTS__?.find((event) => event.kind === 1621),
  );
  expect(createdEvent?.tags).toContainEqual([
    "subject",
    "Document the broken workflow",
  ]);
});

test("selection survives unavailable channel access and can discuss after joining", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  const generalId = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
  await installMockBridge(page, { projectAccessChannelId: generalId });
  await page.goto("/#/projects");
  await page.getByTestId("projects-section-prs").click();
  await page.getByRole("button", { name: "List layout" }).click();
  await page
    .locator('[data-testid^="projects-pr-row-"]')
    .first()
    .getByTestId("projects-row-select")
    .click();
  await page.evaluate(
    async ({ channelId, pubkey }) => {
      window.__BUZZ_E2E_MUTATE_CHANNEL__?.({
        channelId,
        removeMemberPubkey: pubkey,
      });
      await window.__BUZZ_E2E_INVALIDATE_CHANNELS__?.();
    },
    { channelId: generalId, pubkey: DEFAULT_MOCK_PUBKEY },
  );
  const draftsBefore = await page.evaluate(() =>
    Object.entries(localStorage).filter(([key]) =>
      key.startsWith("buzz-drafts."),
    ),
  );
  await page.getByTestId("projects-selection-chat-agent").click();
  await expect(
    page.getByText("No accessible channel is linked to this repository."),
  ).toBeVisible();
  await expect(page.getByTestId("projects-selection-summary")).toContainText(
    "1 review",
  );
  await expect(page).toHaveURL(/#\/projects$/);
  await page.getByTestId("projects-selection-discuss").click();
  await expect(
    page.getByTestId("projects-selection-related-channel"),
  ).toBeDisabled();
  expect(
    await page.evaluate(() =>
      Object.entries(localStorage).filter(([key]) =>
        key.startsWith("buzz-drafts."),
      ),
    ),
  ).toEqual(draftsBefore);
  await page.getByTestId("projects-selection-search-channels").click();
  await page.getByTestId("channel-browser-search").fill("general");
  const channel = page.getByTestId("browse-channel-general");
  await expect(channel).toBeVisible();
  // Choosing an unjoined directory result must fail before any draft write.
  await channel.getByRole("button").first().click();
  await expect(page.getByTestId("projects-selection-summary")).toContainText(
    "1 review",
  );
  await expect(page).toHaveURL(/#\/projects$/);
  expect(
    await page.evaluate(() =>
      Object.entries(localStorage).filter(([key]) =>
        key.startsWith("buzz-drafts."),
      ),
    ),
  ).toEqual(draftsBefore);
  await page.getByTestId("projects-selection-search-channels").click();
  await page.getByTestId("channel-browser-search").fill("general");
  await channel.hover();
  await channel.getByRole("button", { name: "Join", exact: true }).click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("message-input")).toContainText(
    "Let's talk about this review:",
  );
  const commands = await page.evaluate(
    () => window.__BUZZ_E2E_COMMANDS__ ?? [],
  );
  expect(commands).toContain("join_channel");
  expect(commands).not.toContain("send_channel_message");
});

test("selection remains available while channel membership is loading", async ({
  page,
}) => {
  await enableProjectsFeature(page);
  await installMockBridge(page, {
    projectAccessChannelId: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
    channelsReadDelayMs: 8_000,
  });
  await page.goto("/#/projects");
  await page.getByTestId("projects-section-prs").click();
  await page.getByRole("button", { name: "List layout" }).click();
  await page
    .locator('[data-testid^="projects-pr-row-"]')
    .first()
    .getByTestId("projects-row-select")
    .click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__BUZZ_E2E_QUERY_CLIENT__?.getQueryState(["channels"])?.status,
      ),
    )
    .toBe("pending");
  await page.getByTestId("projects-selection-chat-agent").click();
  await expect(
    page.getByText("Channels are still loading. Try again shortly."),
  ).toBeVisible();
  await expect(page.getByTestId("projects-selection-summary")).toContainText(
    "1 review",
  );
  await page.getByTestId("projects-selection-discuss").click();
  await expect(
    page.getByTestId("projects-selection-related-channel"),
  ).toBeDisabled();
  await expect(
    page.getByTestId("projects-selection-related-channel"),
  ).toBeEnabled({ timeout: 12_000 });
  await page.getByTestId("projects-selection-chat-agent").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
  await expect(page.getByTestId("message-input")).toContainText(
    "Let's talk about this review:",
  );
});

async function requiredBox(locator: Locator) {
  const box = await locator.boundingBox();
  if (!box) throw new Error(`Expected rendered bounds for ${locator}`);
  return box;
}

async function expectSinglePrimaryTextColumn(row: Locator) {
  const primary = row.locator('[data-projects-text-priority="primary"]');
  const secondary = row.locator('[data-projects-text-priority="secondary"]');
  await expect(primary).toHaveCount(1);
  expect(await secondary.count()).toBeGreaterThan(0);
  const primaryColor = await primary.evaluate(
    (element) => getComputedStyle(element).color,
  );
  const secondaryColors = await secondary.evaluateAll((elements) =>
    elements.map((element) => getComputedStyle(element).color),
  );
  expect(secondaryColors.every((color) => color !== primaryColor)).toBe(true);
}
