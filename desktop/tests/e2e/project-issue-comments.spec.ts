import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";
import { expandProjectPlumbing } from "../helpers/projectPlumbing";

const ISSUE_COMMENTS = [
  "First issue comment",
  "Second issue comment",
  "Third issue comment",
  "Fourth issue comment",
];

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

test("issue communication actions preserve selected task context in its channel draft", async ({
  page,
}) => {
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  const issueRow = page.getByTestId("project-issue-row").first();
  await expect(issueRow).toBeVisible({ timeout: 10_000 });
  const issueId = await issueRow.getAttribute("data-project-event-id");
  expect(issueId).toBeTruthy();
  await issueRow.getByRole("button", { name: /^#/ }).click();

  const communication = page.getByTestId(
    "project-context-communication-actions",
  );
  await expect(communication).toBeVisible();
  const contextPanel = page.getByTestId("project-repository-actions-panel");
  await expect(
    contextPanel.getByRole("heading", { name: "Actions", exact: true }),
  ).toHaveCount(0);
  await expect(
    contextPanel.getByRole("heading", { name: "Details", exact: true }),
  ).toBeVisible();
  await expect(
    contextPanel.getByRole("heading", { name: "Assignment", exact: true }),
  ).toHaveCount(0);
  await expect(
    contextPanel.getByRole("heading", { name: "Discussion", exact: true }),
  ).toHaveCount(0);
  await expect(
    contextPanel.getByTestId("project-repository-people"),
  ).toHaveCount(0);
  const assertTaskDraft = async () => {
    await expect(page.getByTestId("chat-title")).toHaveText("buzz");
    const composer = page.getByTestId("message-input");
    await expect(composer).toContainText("Let's talk about this task:");
    const chip = composer.locator('[data-composer-buzz-link=""]');
    await expect(chip).toHaveCount(1);
    await expect(chip).toHaveAttribute(
      "data-href",
      new RegExp(`id=${issueId}`),
    );
  };
  await page.getByTestId("project-context-chat-agent").click();
  await assertTaskDraft();
  await page.getByRole("button", { name: "Go back", exact: true }).click();
  await expandProjectPlumbing(page);
  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  await page
    .locator(
      `[data-testid="project-issue-row"][data-project-event-id="${issueId}"]`,
    )
    .getByRole("button", { name: /^#/ })
    .click();
  await expect(communication).toBeVisible();
  await page.getByTestId("project-context-discuss").click();
  const choices = page.getByTestId("project-context-channel-choices");
  await expect(choices).toBeVisible();
  await choices.getByTestId("project-context-related-channel").first().click();
  await assertTaskDraft();
  await page.getByTestId("channel-random").click();
  await expect(
    page.getByTestId("message-input").locator('[data-composer-buzz-link=""]'),
  ).toHaveCount(0);
});

test("issue discussion ignores an author-claimed origin channel", async ({
  page,
}) => {
  const forgedIssueId = "f".repeat(64);
  await page.addInitScript(
    ({ issueId, owner }) => {
      window.__BUZZ_E2E_EXTRA_PROJECT_EVENTS__ = [
        {
          id: issueId,
          kind: 1621,
          pubkey: owner,
          created_at: Math.floor(Date.now() / 1000) + 10,
          content: "This task claims an unrelated visible channel.",
          sig: "mocksig-forged-origin-task",
          tags: [
            ["a", `30617:${owner}:buzz`],
            ["subject", "Forged origin task"],
            ["h", "9dae0116-799b-5071-a0a8-fdd30a91a35d"],
          ],
        },
      ];
    },
    { issueId: forgedIssueId, owner: DEFAULT_MOCK_PUBKEY },
  );
  await installMockBridge(page);
  await openBuzzProject(page);
  await page.getByRole("tab", { name: "Tasks", exact: true }).click();

  const issueRow = page
    .getByTestId("project-issue-row")
    .filter({ hasText: "Forged origin task" });
  await expect(issueRow).toBeVisible();
  await issueRow.getByRole("button", { name: /^#/ }).click();

  await page.getByTestId("project-context-discuss").click();
  const channelChoices = page.getByTestId("project-context-channel-choices");
  const relatedChannel = channelChoices.getByTestId(
    "project-context-related-channel",
  );
  await expect(relatedChannel).toHaveCount(1);
  await expect(relatedChannel).toContainText("#buzz");
  await expect(channelChoices).not.toContainText("#random");
  await relatedChannel.click();

  await expect(page.getByTestId("chat-title")).toHaveText("buzz");
  const issueDraftChip = page
    .getByTestId("message-input")
    .locator('[data-composer-buzz-link=""]', {
      hasText: "buzz",
    });
  await expect(issueDraftChip).toHaveAttribute(
    "data-href",
    new RegExp(`id=${forgedIssueId}`),
  );
  await page.getByTestId("channel-random").click();
  await expect(
    page.getByTestId("message-input").locator('[data-composer-buzz-link=""]'),
  ).toHaveCount(0);
});

test("issue comments use the project activity timeline", async ({ page }) => {
  await installMockBridge(page);
  await openBuzzProject(page);

  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  const issueRow = page.getByTestId("project-issue-row").first();
  await expect(issueRow).toBeVisible({ timeout: 10_000 });
  await issueRow.getByRole("button", { name: /^#/ }).click();

  const composer = page.getByTestId("project-issue-comment-composer");
  await expect(composer).toBeVisible();

  for (const comment of ISSUE_COMMENTS) {
    await composer.locator('[contenteditable="true"]').fill(comment);
    await composer.getByRole("button", { name: "Send message" }).click();
    await expect(page.getByText(comment, { exact: true })).toBeVisible({
      timeout: 10_000,
    });
    const successToast = page.getByText("Comment posted.", { exact: true });
    await expect(successToast).toBeVisible();
    await page.mouse.move(0, 0);
    await expect(successToast).toBeHidden({ timeout: 10_000 });
  }

  const timelineRows = page.getByTestId("project-issue-comment-timeline-row");
  const earlierComments = page.getByTestId("project-issue-earlier-comments");
  const historyToggle = page.getByTestId(
    "project-issue-comment-history-toggle",
  );

  await expect(timelineRows).toHaveCount(3);
  await expect(earlierComments).toContainText("Show 1 earlier comment");
  await expect(
    timelineRows.filter({ hasText: "First issue comment" }),
  ).toHaveCount(0);
  await expect(
    timelineRows.filter({ hasText: "Fourth issue comment" }),
  ).toHaveCount(1);

  await earlierComments.click();
  await expect(timelineRows).toHaveCount(4);
  for (const comment of ISSUE_COMMENTS) {
    await expect(timelineRows.filter({ hasText: comment })).toHaveCount(1);
  }

  await historyToggle.click();
  await expect(timelineRows).toHaveCount(0);
  await expect(historyToggle).toContainText("Show 4 earlier comments");

  await historyToggle.click();
  await expect(timelineRows).toHaveCount(4);
});

const DEFAULT_MOCK_PUBKEY = "deadbeef".repeat(8);
