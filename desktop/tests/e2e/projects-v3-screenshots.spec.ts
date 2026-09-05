import { expect, type Locator, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const SHOTS = "test-results/projects-v3-screenshots";

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

async function expectProjectContextGroups(
  panel: Locator,
  { hasActions }: { hasActions: boolean },
) {
  const detailsHeading = panel.getByRole("heading", {
    name: "Details",
    exact: true,
  });
  await expect(detailsHeading).toBeVisible();
  if (hasActions) {
    await expect(panel.getByTestId("project-context-actions")).toBeVisible();
  } else {
    await expect(panel.getByTestId("project-context-actions")).toHaveCount(0);
  }
  for (const name of [
    "Actions",
    "Assignment",
    "Discussion",
    "People",
    "Task details",
    "Review details",
    "Review activity",
    "Repository activity",
  ]) {
    await expect(panel.getByRole("heading", { name, exact: true })).toHaveCount(
      0,
    );
  }
  await expect(panel.getByTestId("project-repository-people")).toHaveCount(0);
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
  await page.getByTestId("project-plumbing").locator("summary").click();
}

test("repository-only relays keep the Repositories section available", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.__BUZZ_E2E_REPOSITORY_ONLY_PROJECTS__ = true;
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  await expect(
    page.getByRole("group", { name: "Project owner filter" }),
  ).toBeVisible();
  await page.getByTestId("projects-section-repositories").click();
  await expect(
    page.locator(
      '[data-testid="repository-card-buzz"], [data-testid="repository-row-buzz"]',
    ),
  ).toBeVisible();
});

test("projects activity overview screenshot", async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem("buzz-theme", "light");
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");
  await expect(
    page.getByRole("group", { name: "Project owner filter" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Projects", exact: true }),
  ).toBeVisible();
  const landing = page.getByTestId("projects-outcome-landing");
  await expect(
    landing.getByRole("heading", { name: "What needs attention?" }),
  ).toBeVisible();
  await expect(landing).toContainText("Projects, organized by outcome.");
  await expect(
    landing.getByRole("button", { name: "Open project buzz", exact: true }),
  ).toBeVisible();
  await expect(
    landing.getByRole("heading", { name: "buzz", exact: true }),
  ).toBeVisible();
  await expect(landing).toContainText("2 repositories");
  await expect(page.getByTestId("channel-general")).toBeVisible();
  await expect(page.getByTestId("projects-section-all")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/00-projects-pulse.png` });
});

test("project discovery keeps the channel-first sidebar", async ({ page }) => {
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByTestId("channel-general")).toBeVisible();
  await expect(page.getByTestId("sidebar-projects-create")).toHaveCount(0);
  await expect(page.getByTestId("open-projects-view")).toHaveCount(0);

  await page.goto("/#/projects");
  const landing = page.getByTestId("projects-outcome-landing");
  await expect(landing).toBeVisible();
  await landing
    .locator('[data-testid^="project-outcome-card-"]')
    .first()
    .getByRole("button")
    .click();
  await expect(page.getByTestId("project-outcome-page")).toBeVisible();
  await expect(page.getByTestId("sidebar-project-buzz")).toHaveCount(0);
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText("general");
});

test("restricted repositories keep event work visible and offer access help", async ({
  page,
}) => {
  await page.addInitScript((owner) => {
    window.__BUZZ_E2E_PROJECT_OWNER_OVERRIDE__ = owner;
  }, TEST_IDENTITIES.alice.pubkey);
  await installMockBridge(page, {
    projectAccessChannelId: "11111111-1111-4111-8111-111111111111",
    projectRepoSnapshotError: "remote: repository not found",
  });
  await openBuzzProject(page);

  const unavailableState = page
    .getByTestId("project-repository-unavailable")
    .first();
  await expect(unavailableState).toContainText("Repository access restricted");
  await expect(
    unavailableState.getByTestId("repository-owner-name"),
  ).toHaveText("alice");
  await expect(unavailableState).toContainText(/the repository owner/i);
  await expect(
    unavailableState
      .locator(
        '[data-testid="repository-owner-avatar-image"], [data-testid="repository-owner-avatar-fallback"]',
      )
      .first(),
  ).toBeVisible();
  await expect(page.getByTestId("project-section-header")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Ask for access" }),
  ).toHaveCount(2);
  await expect(
    page.getByRole("button", { name: "Clone", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByTestId("project-repository-file-count")).toHaveText(
    "—",
  );

  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  await expect(page.getByTestId("project-issue-row").first()).toBeVisible();

  await page.getByRole("tab", { name: "Review", exact: true }).click();
  await expect(
    page.getByTestId("project-pull-request-row").first(),
  ).toBeVisible();

  await page.getByRole("tab", { name: "Files", exact: true }).click();
  await expect(page.getByText("Repository access restricted")).toBeVisible();
  await expect(page.getByTestId("project-section-header")).toHaveCount(0);

  await page.getByRole("tab", { name: "Commits", exact: true }).click();
  await expect(page.getByText("Repository access restricted")).toBeVisible();
  await page.getByRole("button", { name: "Ask for access" }).first().click();

  await expect(
    page.getByTestId("project-repository-unavailable").first(),
  ).toBeVisible();
  await expect(
    page.getByText("No accessible channel is linked to this repository."),
  ).toBeVisible();
  await expect(
    page.getByText("This project's channel could not be found."),
  ).toHaveCount(0);
});

test("restricted repository access help uses an accessible linked project channel", async ({
  page,
}) => {
  await page.addInitScript((owner) => {
    window.__BUZZ_E2E_PROJECT_OWNER_OVERRIDE__ = owner;
  }, TEST_IDENTITIES.alice.pubkey);
  await installMockBridge(page, {
    projectAccessChannelId: "11111111-1111-4111-8111-111111111111",
    projectHomeChannelId: "cf63feec-21bb-5bf0-a2f8-0e4c3de8ec73",
    projectRepoSnapshotError: "remote: repository not found",
  });
  await openBuzzProject(page);
  await page.getByRole("tab", { name: "Commits", exact: true }).click();
  await expect(page.getByText("Repository access restricted")).toBeVisible();
  await page.getByRole("button", { name: "Ask for access" }).first().click();
  const accessComposer = page.getByTestId("message-input");
  await expect(accessComposer).toBeVisible();
  await expect(
    accessComposer.locator('[data-composer-buzz-link=""]'),
  ).toHaveAttribute("data-href", /[?&]tab=commits(?:&|$)/);
  await expect(accessComposer).toContainText(
    "Let's talk about this repository:",
  );
  await waitForAnimations(page);
  await page.getByTestId("message-composer").screenshot({
    path: `${SHOTS}/08-access-help-channel-draft.png`,
  });
});

test("repository pages show a centered Buzz loader while fetching", async ({
  page,
}) => {
  await installMockBridge(page, { projectRepoSnapshotDelayMs: 750 });
  await openBuzzProject(page);

  const loader = page.getByTestId("buzz-loading-state");
  await expect(loader).toBeVisible();
  await expect(
    loader.getByRole("img", { name: "Loading repository" }),
  ).toBeVisible();
  const animatedMark = loader.locator(".buzz-logo__mark");
  await expect(animatedMark).toHaveCSS(
    "animation-name",
    "buzz-logo-scale-pulse",
  );
  await expect(animatedMark).toHaveCSS("opacity", "1");
  await expect(loader).toHaveCSS("justify-content", "center");
  await expect(loader).toBeHidden({ timeout: 5_000 });
});

// Walks the Projects v3 workspace through its headline states so PR
// screenshots capture distinct pixels per feature (overview box, tab-strip
// plus, issue detail with inline copy link + avatar timeline, PR detail).
test("projects v3 workspace screenshot states", async ({ page }) => {
  await installMockBridge(page);
  await openBuzzProject(page);
  const initialProjectBreadcrumb = page.getByRole("navigation", {
    name: "Project breadcrumb",
  });
  await expect(
    initialProjectBreadcrumb.getByTestId("project-breadcrumb-project"),
  ).toBeVisible();
  await expect(
    initialProjectBreadcrumb.getByTestId("project-breadcrumb-repository"),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Open Discussion" }),
  ).toHaveCount(0);

  // Borderless workspace: repository controls live in the persistent,
  // resizable auxiliary panel instead of a header row.
  const backButton = page.getByTestId("project-workspace-back");
  const overviewTab = page.getByRole("tab", { name: "Overview" });
  const filesTab = page.getByRole("tab", { name: "Files" });
  const channelsTab = page.getByRole("tab", { name: "Channels" });
  const contributorsTab = page.getByRole("tab", { name: "Contributors" });
  const tabMenu = page.getByTestId("project-workspace-tab-menu");
  const workspacePanel = page.getByTestId("project-workspace-panel");
  const projectDetailScroll = page.getByTestId("project-detail-scroll");
  const repositoryActionsPanel = page.getByTestId(
    "project-repository-actions-panel",
  );
  const expectInsetSection = async () => {
    const sectionHeader = workspacePanel.getByTestId("project-section-header");
    const [workspaceBox, headerBox, menuBox] = await Promise.all([
      workspacePanel.boundingBox(),
      sectionHeader.boundingBox(),
      tabMenu.boundingBox(),
    ]);
    expect(workspaceBox).not.toBeNull();
    expect(headerBox).not.toBeNull();
    expect(menuBox).not.toBeNull();
    expect(Math.round((headerBox?.x ?? 0) - (workspaceBox?.x ?? 0))).toBe(16);
    expect(
      Math.round(
        (workspaceBox?.x ?? 0) +
          (workspaceBox?.width ?? 0) -
          ((headerBox?.x ?? 0) + (headerBox?.width ?? 0)),
      ),
    ).toBe(16);
    expect(workspaceBox?.x).toBe(menuBox?.x);
    expect((workspaceBox?.x ?? 0) + (workspaceBox?.width ?? 0)).toBe(
      (menuBox?.x ?? 0) + (menuBox?.width ?? 0),
    );
  };
  await expect(overviewTab).toBeVisible();
  await expect(filesTab).toBeVisible();
  expect((await overviewTab.boundingBox())?.x).toBeLessThan(
    (await filesTab.boundingBox())?.x ?? 0,
  );
  await expect(backButton).toBeVisible();
  await expect(page.getByTestId("app-sidebar")).toBeVisible();
  await expect(projectDetailScroll).toHaveCSS("overscroll-behavior-y", "none");
  await filesTab.click();
  await expect(filesTab).toHaveAttribute("data-state", "active");
  const [projectDetailScrollBounds, initialTabMenuBounds] = await Promise.all([
    projectDetailScroll.boundingBox(),
    tabMenu.boundingBox(),
  ]);
  expect(projectDetailScrollBounds).not.toBeNull();
  expect(initialTabMenuBounds?.y).toBeGreaterThan(
    projectDetailScrollBounds?.y ?? 0,
  );
  await expect(page.getByTestId("project-plumbing")).toHaveAttribute(
    "open",
    "",
  );
  expect(
    await filesTab.evaluate((element) =>
      Number.parseFloat(getComputedStyle(element).borderRadius),
    ),
  ).toBeGreaterThan(13);
  expect((await filesTab.boundingBox())?.height).toBe(28);
  await expect(repositoryActionsPanel).toBeVisible();
  const projectContextCard = repositoryActionsPanel.getByTestId(
    "project-context-card",
  );
  await expect(projectContextCard).toBeVisible();
  await expect(projectContextCard).toHaveCSS("border-radius", "16px");
  const repositoryHeading = repositoryActionsPanel.getByRole("heading", {
    name: "buzz",
    exact: true,
  });
  await expect(repositoryHeading).toBeVisible();
  await expect(repositoryHeading).toHaveCSS("font-size", "14px");
  await expectProjectContextGroups(repositoryActionsPanel, {
    hasActions: true,
  });
  await expect(
    repositoryActionsPanel.getByRole("heading", {
      name: "Languages",
      exact: true,
    }),
  ).toHaveCount(0);
  const detailsHeading = repositoryActionsPanel.getByRole("heading", {
    name: "Details",
    exact: true,
  });
  const fetchButton = repositoryActionsPanel.getByRole("button", {
    name: "Fetch",
    exact: true,
  });
  await expect(detailsHeading.locator("..")).toHaveCSS(
    "border-top-width",
    "0px",
  );
  expect(
    await repositoryHeading.evaluate(
      (element) => getComputedStyle(element).color,
    ),
  ).toBe(
    await detailsHeading.evaluate((element) => getComputedStyle(element).color),
  );
  const actionsSection = repositoryActionsPanel.getByTestId(
    "project-context-actions",
  );
  const detailsSection = repositoryActionsPanel.getByTestId(
    "project-context-details",
  );
  const [actionsSectionBounds, detailsSectionBounds, detailsHeadingBounds] =
    await Promise.all([
      actionsSection.boundingBox(),
      detailsSection.boundingBox(),
      detailsHeading.boundingBox(),
    ]);
  expect(
    (detailsSectionBounds?.y ?? 0) -
      ((actionsSectionBounds?.y ?? 0) + (actionsSectionBounds?.height ?? 0)),
  ).toBe(8);
  expect((detailsHeadingBounds?.y ?? 0) - (detailsSectionBounds?.y ?? 0)).toBe(
    8,
  );
  await expect(fetchButton).toHaveCSS("text-align", "left");
  const sourceButton = repositoryActionsPanel.getByRole("button", {
    name: "Remote",
    exact: true,
  });
  const latestDetailRow = repositoryActionsPanel
    .getByText("Latest", { exact: true })
    .locator("..");
  const [
    alignedDetailsHeadingBounds,
    sourceButtonBounds,
    sourceSectionBounds,
    repositoryActionListBounds,
    sourceTrailingIconBounds,
    fetchButtonBounds,
    fetchIconBounds,
    branchTrailingIconBounds,
    latestDetailRowBounds,
    latestDetailValueBounds,
    managementActionsBounds,
  ] = await Promise.all([
    detailsHeading.boundingBox(),
    sourceButton.boundingBox(),
    repositoryActionsPanel
      .getByTestId("project-context-source-controls")
      .boundingBox(),
    repositoryActionsPanel
      .getByTestId("project-context-repository-actions")
      .boundingBox(),
    sourceButton.locator("svg").last().boundingBox(),
    fetchButton.boundingBox(),
    fetchButton.locator("svg").first().boundingBox(),
    repositoryActionsPanel
      .getByTestId("project-repository-branch-trigger")
      .locator("svg")
      .last()
      .boundingBox(),
    latestDetailRow.boundingBox(),
    latestDetailRow.locator("dd").boundingBox(),
    repositoryActionsPanel
      .getByTestId("project-repository-management-actions")
      .boundingBox(),
  ]);
  expect(alignedDetailsHeadingBounds?.height).toBe(28);
  expect(sourceButtonBounds?.height).toBe(28);
  expect(fetchButtonBounds?.height).toBe(28);
  expect(latestDetailRowBounds?.height).toBe(28);
  expect(
    (repositoryActionListBounds?.y ?? 0) -
      ((sourceSectionBounds?.y ?? 0) + (sourceSectionBounds?.height ?? 0)),
  ).toBe(2);
  expect(fetchButtonBounds?.x ?? 0).toBeLessThan(
    alignedDetailsHeadingBounds?.x ?? 0,
  );
  expect(fetchIconBounds?.x).toBe(alignedDetailsHeadingBounds?.x);
  const detailRight =
    (latestDetailValueBounds?.x ?? 0) + (latestDetailValueBounds?.width ?? 0);
  expect(
    (sourceTrailingIconBounds?.x ?? 0) + (sourceTrailingIconBounds?.width ?? 0),
  ).toBe(detailRight);
  expect(
    (branchTrailingIconBounds?.x ?? 0) + (branchTrailingIconBounds?.width ?? 0),
  ).toBe(detailRight);
  expect(
    (managementActionsBounds?.x ?? 0) + (managementActionsBounds?.width ?? 0),
  ).toBe(detailRight);
  await expect(
    repositoryActionsPanel.getByText("Working copy", { exact: true }),
  ).toHaveCount(0);
  await expect(
    repositoryActionsPanel.getByTestId("project-right-panel-scope"),
  ).toHaveCount(0);
  // Crew places repository context beside the workspace inside its project
  // outcome page. Verify the live surfaces fit without the old detached rail.
  await expect(repositoryActionsPanel).toBeVisible();
  await expect(workspacePanel).toBeVisible();
  const viewport = page.viewportSize();
  for (const surface of [workspacePanel, repositoryActionsPanel]) {
    const bounds = await surface.boundingBox();
    expect(bounds).not.toBeNull();
    if (!bounds || !viewport)
      throw new Error("Project layout was not measured.");
    expect(bounds.width).toBeGreaterThan(0);
    expect(bounds.x + bounds.width).toBeLessThanOrEqual(viewport.width);
  }
  await expect(page.getByTestId("project-detail-copy-link")).toHaveCount(0);
  await expect(
    workspacePanel.getByTestId("project-workspace-tab-menu"),
  ).toHaveCount(0);
  const backBounds = await backButton.boundingBox();
  const channelsBounds = await channelsTab.boundingBox();
  const contributorsBounds = await contributorsTab.boundingBox();
  const tabMenuBounds = await tabMenu.boundingBox();
  const workspaceBounds = await workspacePanel.boundingBox();
  const repositoryActionsBounds = await repositoryActionsPanel.boundingBox();
  expect(backBounds).not.toBeNull();
  expect(channelsBounds).not.toBeNull();
  expect(contributorsBounds).not.toBeNull();
  expect(tabMenuBounds).not.toBeNull();
  expect(workspaceBounds).not.toBeNull();
  expect(repositoryActionsBounds).not.toBeNull();
  expect(channelsBounds?.x).toBeLessThan(contributorsBounds?.x ?? 0);
  expect(backBounds?.x).toBeGreaterThan(workspaceBounds?.x ?? 0);
  expect(tabMenuBounds?.y).toBeLessThan(workspaceBounds?.y ?? 0);
  expect(repositoryActionsBounds?.x).toBeGreaterThan(workspaceBounds?.x ?? 0);
  await expect(
    workspacePanel.getByRole("heading", { name: "README", exact: true }),
  ).toHaveCount(0);
  await expect(
    workspacePanel.getByTestId("project-section-header-icon"),
  ).toHaveCount(1);
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/01-workspace-overview.png` });

  await repositoryActionsPanel
    .getByRole("button", { name: "Clone", exact: true })
    .click();
  const localSourceTrigger = repositoryActionsPanel.getByRole("button", {
    name: /^Local /,
  });
  await expect(localSourceTrigger).toBeVisible();
  await expect(
    repositoryActionsPanel.getByTestId("project-repository-local-path"),
  ).toHaveText("…/buzz/REPOS/mock-project");
  await expect(
    repositoryActionsPanel.getByRole("button", {
      name: "Open",
      exact: true,
    }),
  ).toHaveAttribute("title", "Open local repository folder");
  await localSourceTrigger.click();
  await page
    .getByRole("menuitemradio", { name: "Remote", exact: true })
    .click();
  await expect(
    repositoryActionsPanel.getByRole("button", {
      name: "Remote",
      exact: true,
    }),
  ).toBeVisible();

  await filesTab.click();
  await expect(repositoryActionsPanel).toBeVisible();
  await expect(
    workspacePanel.getByRole("heading", { name: "Files", exact: true }),
  ).toBeVisible();
  await expect(
    workspacePanel.getByTestId("project-repository-entry-icon").first(),
  ).toHaveCSS("border-radius", "8px");
  const repositoryEntryCell = workspacePanel
    .getByTestId("project-repository-entry-row")
    .first()
    .locator("td")
    .first();
  await expect(repositoryEntryCell).toHaveCSS("border-radius", "0px");
  await expect(repositoryEntryCell).toHaveCSS("border-bottom-width", "0px");
  await expectInsetSection();
  await page.getByRole("tab", { name: "Commits" }).click();
  await expect(repositoryActionsPanel).toBeVisible();
  await expect(
    workspacePanel.getByRole("heading", { name: "Commits", exact: true }),
  ).toBeVisible();
  await expectInsetSection();
  const commitRow = page.getByTestId("project-activity-feed-item").first();
  const commitDate = commitRow.getByTestId("project-commit-row-date");
  const commitHash = commitRow.getByTitle(/^View commit /);
  await expect(commitDate).toBeVisible();
  const commitDateBounds = await commitDate.boundingBox();
  const commitHashBounds = await commitHash.boundingBox();
  expect(commitDateBounds).not.toBeNull();
  expect(commitHashBounds).not.toBeNull();
  expect(commitDateBounds?.x).toBeGreaterThan(commitHashBounds?.x ?? 0);
  await contributorsTab.click();
  await expectInsetSection();
  const contributorRow = page.getByTestId("project-contributor-row").first();
  await expect(
    contributorRow.getByTestId("project-contributor-commit-count"),
  ).toBeVisible();
  await expect(
    contributorRow.getByTestId("project-contributor-review-count"),
  ).toBeVisible();
  await expect(
    contributorRow.getByTestId("project-contributor-task-count"),
  ).toBeVisible();
  const contributorBounds = await contributorRow.boundingBox();
  const contributorIdentityBounds = await contributorRow
    .getByTestId("project-contributor-identity")
    .boundingBox();
  const contributorCommitCountBounds = await contributorRow
    .getByTestId("project-contributor-commit-count")
    .boundingBox();
  expect(contributorBounds).not.toBeNull();
  expect(contributorBounds?.height).toBeLessThanOrEqual(40);
  expect(contributorIdentityBounds).not.toBeNull();
  expect(contributorCommitCountBounds?.x).toBeGreaterThan(
    contributorIdentityBounds?.x ?? 0,
  );
  const contributorCommitCounts = await page
    .getByTestId("project-contributor-commit-count")
    .evaluateAll((nodes) =>
      nodes.map((node) => {
        const value = Number.parseInt(node.textContent?.trim() ?? "", 10);
        return Number.isNaN(value) ? -1 : value;
      }),
    );
  expect(contributorCommitCounts).toEqual(
    [...contributorCommitCounts].sort((left, right) => right - left),
  );

  await channelsTab.click();
  await expect(
    workspacePanel.getByRole("heading", { name: "Channels", exact: true }),
  ).toBeVisible();
  await expect(
    repositoryActionsPanel.getByTestId("project-right-panel-scope"),
  ).toHaveCount(0);
  await expect(
    repositoryActionsPanel.getByText("Working copy", { exact: true }),
  ).toHaveCount(0);
  await expectProjectContextGroups(repositoryActionsPanel, {
    hasActions: false,
  });
  for (const action of ["Clone", "Fetch", "Terminal"]) {
    await expect(
      repositoryActionsPanel.getByRole("button", {
        name: action,
        exact: true,
      }),
    ).toHaveCount(0);
  }
  await expectInsetSection();

  // Issues tab: the create action lives in the section header.
  await page.getByRole("tab", { name: "Tasks", exact: true }).click();
  await expect(repositoryActionsPanel).toBeVisible();
  await expectProjectContextGroups(repositoryActionsPanel, {
    hasActions: true,
  });
  await expect(
    repositoryActionsPanel.getByRole("heading", {
      name: "Task activity",
      exact: true,
    }),
  ).toHaveCount(0);
  const taskActionsSection = repositoryActionsPanel.getByTestId(
    "project-context-actions",
  );
  const taskDetailsSection = repositoryActionsPanel.getByTestId(
    "project-context-details",
  );
  const [taskActionsBounds, taskDetailsBounds] = await Promise.all([
    taskActionsSection.boundingBox(),
    taskDetailsSection.boundingBox(),
  ]);
  expect(taskDetailsBounds?.y ?? 0).toBeGreaterThan(
    (taskActionsBounds?.y ?? 0) + (taskActionsBounds?.height ?? 0),
  );
  await expectInsetSection();
  const newIssueButton = workspacePanel.getByRole("button", {
    name: "Create task",
  });
  await expect(newIssueButton).toBeVisible();
  const contextCreateTaskButton = repositoryActionsPanel.getByRole("button", {
    name: "Create task",
    exact: true,
  });
  await expect(contextCreateTaskButton).toBeVisible();
  await contextCreateTaskButton.click();
  await expect(page.getByTestId("create-issue-dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    tabMenu.getByRole("button", { name: "Create task" }),
  ).toHaveCount(0);
  const issuesHeading = workspacePanel.getByRole("heading", {
    name: "Tasks",
    exact: true,
  });
  const issuesHeadingBounds = await issuesHeading.boundingBox();
  const newIssueBounds = await newIssueButton.boundingBox();
  expect(issuesHeadingBounds).not.toBeNull();
  expect(newIssueBounds).not.toBeNull();
  expect(newIssueBounds?.x).toBeGreaterThan(issuesHeadingBounds?.x ?? 0);
  const issueRow = page.getByTestId("project-issue-row").first();
  await expect(issueRow).toBeVisible({ timeout: 10_000 });
  await expect(issueRow).toHaveCSS("border-radius", "8px");
  const [issueHeaderBounds, firstIssueBounds, secondIssueBounds] =
    await Promise.all([
      page.getByTestId("project-work-item-group-header").first().boundingBox(),
      page.getByTestId("project-issue-row").nth(0).boundingBox(),
      page.getByTestId("project-issue-row").nth(1).boundingBox(),
    ]);
  expect(
    (firstIssueBounds?.y ?? 0) -
      ((issueHeaderBounds?.y ?? 0) + (issueHeaderBounds?.height ?? 0)),
  ).toBe(4);
  expect(
    Math.round((firstIssueBounds?.x ?? 0) - (issueHeaderBounds?.x ?? 0)),
  ).toBe(8);
  expect(
    Math.round(
      (issueHeaderBounds?.width ?? 0) - (firstIssueBounds?.width ?? 0),
    ),
  ).toBe(16);
  expect(
    (secondIssueBounds?.y ?? 0) -
      ((firstIssueBounds?.y ?? 0) + (firstIssueBounds?.height ?? 0)),
  ).toBe(2);
  const issueDate = issueRow.getByTestId("project-issue-row-date");
  const issueId = issueRow.getByTitle("View task");
  await expect(issueDate).toBeVisible();
  const issueDateBounds = await issueDate.boundingBox();
  const issueIdBounds = await issueId.boundingBox();
  const issueRowBounds = await issueRow.boundingBox();
  expect(issueDateBounds).not.toBeNull();
  expect(issueIdBounds).not.toBeNull();
  expect(issueRowBounds).not.toBeNull();
  expect(issueRowBounds?.height).toBeLessThanOrEqual(40);
  expect(issueDateBounds?.x).toBeGreaterThan(issueIdBounds?.x ?? 0);
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/02-issues-tab.png` });

  // Issue detail: inline copy link after the title, assignees rail, and the
  // avatar comment timeline (seed comments so the timeline renders).
  await issueRow.getByRole("button", { name: /^#/ }).click();
  const composer = page.getByTestId("project-issue-comment-composer");
  await expect(composer).toBeVisible();
  await expect(tabMenu).toHaveCount(0);
  for (const comment of [
    "The palette needs the most work — starting there.",
    "Agreed. Typography pass can land separately.",
  ]) {
    await composer.locator('[contenteditable="true"]').fill(comment);
    await composer.getByRole("button", { name: "Send message" }).click();
    await expect(page.getByText(comment, { exact: true })).toBeVisible({
      timeout: 10_000,
    });
  }
  await expect(page.getByTestId("project-issue-copy-link")).toBeVisible();
  await expectProjectContextGroups(repositoryActionsPanel, {
    hasActions: true,
  });
  const issueDetail = page.getByTestId("project-issue-detail");
  await expect(issueDetail).toHaveCSS("max-width", "768px");
  await expect(
    issueDetail.getByRole("heading", { level: 3 }).first(),
  ).toHaveCSS("font-size", "18px");
  await expect(
    page.getByTestId("project-issue-comment-timeline-row").first(),
  ).toBeVisible();
  const issueActivity = workspacePanel.getByRole("button", {
    name: "Activity",
    exact: true,
  });
  await issueActivity.click();
  await expect(composer).toBeVisible();
  await expect(
    page.getByTestId("project-issue-comment-timeline-row"),
  ).toHaveCount(0);
  await issueActivity.click();
  // Let the "Comment posted." toast dismiss so it doesn't photobomb.
  await expect(page.getByText("Comment posted.")).toHaveCount(0, {
    timeout: 10_000,
  });
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/03-issue-detail.png` });

  // PR list: the create action lives in both the section header and context.
  await page
    .getByRole("navigation", { name: "Project breadcrumb" })
    .getByRole("button", { name: "Tasks", exact: true })
    .click();
  await expect(tabMenu).toBeVisible();
  await page.getByRole("tab", { name: "Review", exact: true }).click();
  await expect(
    repositoryActionsPanel.getByTestId("project-right-panel-scope"),
  ).toHaveCount(0);
  await expectProjectContextGroups(repositoryActionsPanel, {
    hasActions: true,
  });
  await expect(
    workspacePanel.getByRole("heading", {
      name: "Reviews",
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    workspacePanel.getByRole("button", { name: "Create review" }),
  ).toBeVisible();
  const contextCreateReviewButton = repositoryActionsPanel.getByRole("button", {
    name: "Create review",
    exact: true,
  });
  await expect(contextCreateReviewButton).toBeVisible();
  await contextCreateReviewButton.click();
  await expect(page.getByTestId("create-pull-request-dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expectInsetSection();
  await expect(
    tabMenu.getByRole("button", { name: "Create review" }),
  ).toHaveCount(0);
  const prRow = page.getByTestId("project-pull-request-row").first();
  await expect(prRow).toBeVisible({ timeout: 10_000 });
  const [reviewHeaderBounds, firstReviewBounds, secondReviewBounds] =
    await Promise.all([
      page.getByTestId("project-work-item-group-header").first().boundingBox(),
      page.getByTestId("project-pull-request-row").nth(0).boundingBox(),
      page.getByTestId("project-pull-request-row").nth(1).boundingBox(),
    ]);
  expect(
    (firstReviewBounds?.y ?? 0) -
      ((reviewHeaderBounds?.y ?? 0) + (reviewHeaderBounds?.height ?? 0)),
  ).toBe(4);
  expect(
    Math.round((firstReviewBounds?.x ?? 0) - (reviewHeaderBounds?.x ?? 0)),
  ).toBe(8);
  expect(
    Math.round(
      (reviewHeaderBounds?.width ?? 0) - (firstReviewBounds?.width ?? 0),
    ),
  ).toBe(16);
  expect(
    (secondReviewBounds?.y ?? 0) -
      ((firstReviewBounds?.y ?? 0) + (firstReviewBounds?.height ?? 0)),
  ).toBe(2);
  const prDate = prRow.getByTestId("project-pull-request-row-date");
  const prId = prRow.getByTitle("View review");
  await expect(prDate).toBeVisible();
  const prDateBounds = await prDate.boundingBox();
  const prIdBounds = await prId.boundingBox();
  expect(prDateBounds).not.toBeNull();
  expect(prIdBounds).not.toBeNull();
  expect(prDateBounds?.x).toBeGreaterThan(prIdBounds?.x ?? 0);
  await prRow.getByRole("button", { name: /^#/ }).click();
  await expect(
    page.getByTestId("project-pull-request-copy-link"),
  ).toBeVisible();
  await expectProjectContextGroups(repositoryActionsPanel, {
    hasActions: true,
  });
  const pullRequestDetail = page.getByTestId("project-pull-request-detail");
  await expect(pullRequestDetail).toHaveCSS("max-width", "768px");
  await expect(
    pullRequestDetail.getByRole("heading", { level: 3 }).first(),
  ).toHaveCSS("font-size", "18px");
  const reviewCommits = workspacePanel.getByRole("button", {
    name: "Commits",
    exact: true,
  });
  await expect(reviewCommits).toHaveAttribute("aria-expanded", "false");
  await expect(reviewCommits).toHaveCSS("font-size", "14px");
  await expect(reviewCommits).toHaveCSS("font-weight", "500");
  await reviewCommits.click();
  const openedReviewCommits = workspacePanel.getByRole("button", {
    name: /^Commits \d+$/,
  });
  await expect(openedReviewCommits).toHaveAttribute("aria-expanded", "true");
  await openedReviewCommits.click();
  const reviewComposer = page.getByTestId(
    "project-pull-request-comment-composer",
  );
  await expect(reviewComposer).toBeVisible();
  const reviewActivity = workspacePanel.getByRole("button", {
    name: "Activity",
    exact: true,
  });
  await reviewActivity.click();
  await expect(reviewComposer).toBeVisible();
  await expect(tabMenu).toHaveCount(0);
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/04-pr-detail.png` });
});

test("projects v3 work-item list metadata", async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem("buzz.projects.viewMode", "list");
  });
  await installMockBridge(page);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.goto("/#/projects");

  await page.getByTestId("projects-section-all").click();
  const outcomeCard = page.getByTestId("project-outcome-card-buzz");
  await expect(
    outcomeCard.getByRole("heading", { name: "buzz", exact: true }),
  ).toBeVisible();
  await expect(outcomeCard).toContainText("2 repositories");

  await page.getByTestId("projects-section-projects").click();
  await expect(page.getByTestId("projects-list-header")).toHaveCSS(
    "border-left-width",
    "0px",
  );
  const projectRow = page.getByTestId(/^project-row-/).first();
  await expectSinglePrimaryTextColumn(projectRow);
  const [projectTitleBox, repositoryCountBox] = await Promise.all([
    projectRow.locator('[data-projects-text-priority="primary"]').boundingBox(),
    projectRow.getByTestId("projects-row-context").boundingBox(),
  ]);
  expect(projectTitleBox).not.toBeNull();
  expect(repositoryCountBox).not.toBeNull();
  expect(
    Math.round(
      (repositoryCountBox?.x ?? 0) -
        ((projectTitleBox?.x ?? 0) + (projectTitleBox?.width ?? 0)),
    ),
  ).toBe(12);

  await page.getByTestId("projects-section-repositories").click();
  await expectSinglePrimaryTextColumn(
    page.getByTestId(/^repository-row-/).first(),
  );

  await page.getByTestId("projects-section-prs").click();
  const reviewList = page.getByTestId("projects-list-container");
  await expect(reviewList).toBeVisible();
  const pullRequestRow = page.getByTestId(/^projects-pr-row-/).first();
  await expect(pullRequestRow).toBeVisible();
  await expectSinglePrimaryTextColumn(pullRequestRow);
  await expect(pullRequestRow).toContainText(/relay-tools|buzz|design-system/);
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/05-pr-list-metadata.png` });

  await page.getByTestId("projects-section-issues").click();
  const taskList = page.getByTestId("projects-list-container");
  await expect(taskList).toBeVisible();
  const issueRow = page.getByTestId(/^projects-issue-row-/).first();
  await expect(issueRow).toBeVisible();
  await expectSinglePrimaryTextColumn(issueRow);
  await expect(issueRow).toContainText(/relay-tools|buzz|design-system/);
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/06-issue-list-metadata.png` });

  await page.getByTestId("projects-section-channels").click();
  const channelRow = page.getByTestId("project-channel-row").first();
  await expect(channelRow).toBeVisible();
  await expectSinglePrimaryTextColumn(channelRow);
  await expect(channelRow).toContainText("#buzz");
  await expect(
    page.getByTestId("project-channel-project").first(),
  ).toBeVisible();
  await expect(
    page.getByTestId("project-channel-repository").first(),
  ).toBeVisible();
  await waitForAnimations(page);
  await page.screenshot({ path: `${SHOTS}/07-channels-list.png` });
});
