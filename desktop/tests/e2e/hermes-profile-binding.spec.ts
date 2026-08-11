/**
 * Hermes profile binding UI (Phase 02B / Phase 04 picker / feature 0001).
 *
 * Pins capability-driven visibility: profileArg runtimes show the binding
 * field + profile-owned model row; goose does not. Invalid names and
 * "default" are blocked client-side. Model is never an editable control.
 * Phase 04: disk profiles appear in a combobox; occupancy blocks save.
 */
import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const HERMES_AGENT_PUBKEY = TEST_IDENTITIES.alice.pubkey;
const GOOSE_AGENT_PUBKEY = TEST_IDENTITIES.tyler.pubkey;
const REMOTE_PROVIDER = {
  id: "kubernetes",
  binaryPath: "/mock/buzz-backend-kubernetes",
};
const REMOTE_PROVIDER_PROBE = {
  ok: true,
  name: "kubernetes",
  version: "0.0.0-mock",
  config_schema: {
    type: "object",
    properties: {},
    required: [],
  },
};

const PRODUCT_COMMUNITY = {
  id: "product",
  name: "Product",
  relayUrl: "ws://localhost:3000",
  addedAt: "2026-08-08T00:00:00.000Z",
};
const RESEARCH_COMMUNITY = {
  id: "research",
  name: "Research",
  relayUrl: "ws://localhost:3001",
  addedAt: "2026-08-08T00:00:01.000Z",
};

async function openCreateCustomize(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("open-agents-view").click();
  await page.getByTestId("new-agent-card").click();
  await page.getByRole("menuitem", { name: "Create agent" }).click();
  const dialog = page.getByTestId("persona-dialog");
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  await dialog.getByRole("tab", { name: "Customize for this agent" }).click();
  return dialog;
}

async function pickRuntime(
  page: import("@playwright/test").Page,
  dialog: import("@playwright/test").Locator,
  label: string | RegExp,
) {
  const harness = dialog.locator("#persona-runtime");
  await expect(harness).toBeVisible({ timeout: 10_000 });
  await harness.click();
  await page.getByRole("menuitemradio", { name: label }).click();
}

async function openEditForAgent(
  page: import("@playwright/test").Page,
  agentName: string,
) {
  await page.goto("/");
  await page.getByTestId("open-agents-view").click();
  const agentButton = page.getByRole("button", {
    name: `${agentName} agent profile`,
  });
  await expect(agentButton).toBeVisible({ timeout: 10_000 });
  await agentButton.click();
  await expect(page.getByTestId("user-profile-panel")).toBeVisible({
    timeout: 10_000,
  });
  await page.getByTestId("user-profile-edit-agent").click();
  await expect(page.getByTestId("edit-agent-dialog")).toBeVisible({
    timeout: 10_000,
  });
}

async function seedCommunities(
  page: import("@playwright/test").Page,
  activeId: string,
) {
  await page.addInitScript(
    ({ communities, active }) => {
      window.localStorage.setItem(
        "buzz-communities",
        JSON.stringify(communities),
      );
      window.localStorage.setItem("buzz-active-community-id", active);
    },
    {
      communities: [PRODUCT_COMMUNITY, RESEARCH_COMMUNITY],
      active: activeId,
    },
  );
}

async function configureExistingHermesProfile(
  page: import("@playwright/test").Page,
  dialog: import("@playwright/test").Locator,
) {
  await pickRuntime(page, dialog, /Hermes Agent/);
  await waitForAnimations(page);
  await dialog.locator("#persona-display-name").fill("Hermes Scout");
  await dialog.locator("#persona-hermes-profile").fill("scout");
}

function createSubmit(dialog: import("@playwright/test").Locator) {
  return dialog.getByRole("button", { name: /Create|Save|Start/i }).first();
}

test.describe("hermes profile binding", () => {
  test("create: Hermes shows profile field and profile-owned model row", async ({
    page,
  }) => {
    await installMockBridge(page);
    const dialog = await openCreateCustomize(page);

    await pickRuntime(page, dialog, /Hermes Agent/);
    await waitForAnimations(page);

    await expect(page.getByTestId("hermes-profile-field")).toBeVisible();
    await expect(page.getByTestId("profile-owned-model-row")).toBeVisible();
    await expect(page.getByTestId("profile-owned-model-row")).toContainText(
      "decided by profile",
    );
    // No editable model control when the profile owns the model.
    await expect(dialog.locator("#persona-model")).toHaveCount(0);
  });

  test("create: switching away clears the hidden Hermes profile", async ({
    page,
  }) => {
    await installMockBridge(page, { hermesProfiles: ["scout"] });
    const dialog = await openCreateCustomize(page);

    await configureExistingHermesProfile(page, dialog);
    await pickRuntime(page, dialog, /^Goose$/);
    await waitForAnimations(page);

    await expect(page.getByTestId("hermes-profile-field")).toHaveCount(0);
    await expect(page.getByTestId("profile-owned-model-row")).toHaveCount(0);

    await pickRuntime(page, dialog, /Hermes Agent/);
    await expect(dialog.locator("#persona-hermes-profile")).toHaveValue("");
  });

  test("create: invalid profile name and default are blocked", async ({
    page,
  }) => {
    await installMockBridge(page);
    const dialog = await openCreateCustomize(page);

    await pickRuntime(page, dialog, /Hermes Agent/);
    await dialog.locator("#persona-display-name").fill("Scout Hermes");

    const profileInput = dialog.locator("#persona-hermes-profile");
    await profileInput.fill("Bad Name");
    await expect(page.getByTestId("hermes-profile-error")).toBeVisible();
    await expect(page.getByTestId("hermes-profile-error")).toContainText(
      /lowercase/i,
    );
    await expect(
      dialog.getByRole("button", { name: /Create|Save|Start/i }).first(),
    ).toBeDisabled();

    await profileInput.fill("default");
    await expect(page.getByTestId("hermes-profile-error")).toContainText(
      /default/i,
    );
    await expect(
      dialog.getByRole("button", { name: /Create|Save|Start/i }).first(),
    ).toBeDisabled();

    await profileInput.fill("scout");
    await expect(page.getByTestId("hermes-profile-error")).toHaveCount(0);
    await expect(page.getByTestId("profile-owned-model-row")).toContainText(
      "decided by profile scout",
    );
  });

  test("create: lists existing disk profiles and pick binds", async ({
    page,
  }) => {
    await installMockBridge(page, {
      hermesProfiles: ["scout", "builder", "default"],
    });
    const dialog = await openCreateCustomize(page);
    await pickRuntime(page, dialog, /Hermes Agent/);
    await waitForAnimations(page);

    await dialog.getByTestId("hermes-profile-combobox-trigger").click();
    await waitForAnimations(page);

    const list = page.getByTestId("hermes-profile-combobox-list");
    await expect(list).toBeVisible();
    await expect(list.getByTestId("hermes-profile-option")).toHaveCount(2);
    await expect(list.getByTestId("hermes-profile-option")).toContainText([
      "builder",
      "scout",
    ]);
    await expect(list.getByText("default", { exact: true })).toHaveCount(0);

    await list
      .getByTestId("hermes-profile-option")
      .filter({ hasText: "scout" })
      .click();
    await expect(dialog.locator("#persona-hermes-profile")).toHaveValue(
      "scout",
    );
    await expect(page.getByTestId("profile-owned-model-row")).toContainText(
      "decided by profile scout",
    );
    // Existing profile — no create-in-place button.
    await expect(page.getByTestId("hermes-profile-create-button")).toHaveCount(
      0,
    );
  });

  test("create: create-in-place button appears for a new valid profile name", async ({
    page,
  }) => {
    await installMockBridge(page, { hermesProfiles: ["scout"] });
    const dialog = await openCreateCustomize(page);

    await pickRuntime(page, dialog, /Hermes Agent/);
    await waitForAnimations(page);

    const profileInput = dialog.locator("#persona-hermes-profile");
    await profileInput.fill("builder");
    await expect(
      page.getByTestId("hermes-profile-create-affordance"),
    ).toBeVisible();
    await expect(
      page.getByTestId("hermes-profile-create-button"),
    ).toContainText("Create profile 'builder'");
    await expect(
      page.getByTestId("hermes-profile-create-affordance"),
    ).toContainText("hermes profile create builder --no-alias");

    await page.getByTestId("hermes-profile-create-button").click();
    await expect(page.getByTestId("hermes-profile-create-button")).toHaveCount(
      0,
      { timeout: 10_000 },
    );

    await dialog.getByTestId("hermes-profile-combobox-trigger").click();
    await waitForAnimations(page);
    await expect(
      page.getByTestId("hermes-profile-option").filter({ hasText: "builder" }),
    ).toBeVisible();
  });

  test("create: bound profile shows occupancy and blocks submit", async ({
    page,
  }) => {
    await installMockBridge(page, {
      hermesProfiles: ["scout", "builder"],
      managedAgents: [
        {
          pubkey: HERMES_AGENT_PUBKEY,
          name: "Hermes Scout",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "hermes",
          hermesProfile: "scout",
        },
      ],
    });
    const dialog = await openCreateCustomize(page);
    await pickRuntime(page, dialog, /Hermes Agent/);
    await waitForAnimations(page);

    await dialog.getByTestId("hermes-profile-combobox-trigger").click();
    await waitForAnimations(page);
    const scoutOption = page
      .getByTestId("hermes-profile-option")
      .filter({ hasText: "scout" });
    await expect(
      scoutOption.getByTestId("hermes-profile-occupancy"),
    ).toContainText(/bound/i);
    await scoutOption.click();

    await expect(page.getByTestId("hermes-profile-error")).toContainText(
      /already bound/i,
    );
    await expect(
      dialog.getByRole("button", { name: /Create|Save|Start/i }).first(),
    ).toBeDisabled();
  });

  test("create: trusted owner-only local full-autonomy boundary is explicit", async ({
    page,
  }) => {
    await installMockBridge(page, {
      hermesProfiles: ["scout"],
      backendProviders: [REMOTE_PROVIDER],
      backendProviderProbeResult: REMOTE_PROVIDER_PROBE,
    });
    const dialog = await openCreateCustomize(page);
    await configureExistingHermesProfile(page, dialog);

    const boundary = dialog.getByTestId("hermes-effective-boundary");
    await expect(boundary).toBeVisible();
    await expect(boundary).toContainText("Access");
    await expect(boundary).toContainText("Owner only");
    await expect(boundary).toContainText("Autonomy");
    await expect(boundary).toContainText("Full");
    await expect(boundary).toContainText("Backend");
    await expect(boundary).toContainText("This Mac");
    await expect(boundary).toContainText("Profile");
    await expect(boundary).toContainText("scout");
  });

  test("create: profile-bound Hermes blocks public access with actionable copy", async ({
    page,
  }) => {
    await installMockBridge(page, { hermesProfiles: ["scout"] });
    const dialog = await openCreateCustomize(page);
    await configureExistingHermesProfile(page, dialog);

    await dialog.getByRole("button", { name: /^Advanced/ }).click();
    await page.locator("#agent-respond-to").click();
    await page.getByRole("menuitemradio", { name: "Anyone" }).click();

    const error = dialog.getByTestId("hermes-trusted-boundary-error");
    await expect(error).toContainText(/owner-only/i);
    await expect(error).toContainText(/choose.*Only me|change.*access/i);
    await expect(createSubmit(dialog)).toBeDisabled();
  });

  test("create: profile-bound Hermes blocks a remote backend with actionable copy", async ({
    page,
  }) => {
    await installMockBridge(page, {
      hermesProfiles: ["scout"],
      backendProviders: [REMOTE_PROVIDER],
      backendProviderProbeResult: REMOTE_PROVIDER_PROBE,
    });
    const dialog = await openCreateCustomize(page);
    await configureExistingHermesProfile(page, dialog);

    await dialog.getByRole("button", { name: /^Advanced/ }).click();
    const runOn = dialog.locator("#agent-run-on");
    await runOn.press("Enter");
    await page
      .getByRole("menuitemradio", { exact: true, name: REMOTE_PROVIDER.id })
      .press("Enter");
    await expect(runOn).toContainText(REMOTE_PROVIDER.id);

    const error = dialog.getByTestId("hermes-trusted-boundary-error");
    await expect(error).toContainText(/local|This Mac/i);
    await expect(error).toContainText(/choose.*This computer|run.*locally/i);
    await expect(createSubmit(dialog)).toBeDisabled();
  });

  test("edit: one profile-bound agent discloses multi-community shared state", async ({
    page,
  }) => {
    await seedCommunities(page, RESEARCH_COMMUNITY.id);
    await installMockBridge(
      page,
      {
        hermesProfiles: ["scout"],
        managedAgents: [
          {
            pubkey: HERMES_AGENT_PUBKEY,
            name: "Hermes Scout",
            status: "stopped",
            channelNames: ["agents"],
            runtime: "hermes",
            hermesProfile: "scout",
          },
        ],
      },
      { skipCommunitySeed: true },
    );
    await openEditForAgent(page, "Hermes Scout");
    const dialog = page.getByTestId("edit-agent-dialog");
    const usage = dialog.getByTestId("hermes-profile-shared-usage");
    await expect(usage).toBeVisible();
    const boundary = dialog.getByTestId("hermes-effective-boundary");
    await expect(boundary).toContainText("Product, Research");
    await expect(usage).toContainText(
      "One managed agent uses this profile across its configured communities.",
    );
    await expect(usage).toContainText(
      "Memory, skills, and profile state are shared.",
    );
  });

  test("edit: switching away clears the hidden Hermes profile", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: HERMES_AGENT_PUBKEY,
          name: "Hermes Scout",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "hermes",
          hermesProfile: "scout",
        },
        {
          pubkey: GOOSE_AGENT_PUBKEY,
          name: "Tyler Agent",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "goose",
        },
      ],
    });

    await openEditForAgent(page, "Hermes Scout");
    await waitForAnimations(page);

    await expect(page.getByTestId("hermes-profile-field")).toBeVisible();
    await expect(page.locator("#edit-agent-hermes-profile")).toHaveValue(
      "scout",
    );
    await expect(page.getByTestId("profile-owned-model-row")).toContainText(
      "decided by profile scout",
    );
    await expect(page.locator("#edit-agent-model")).toHaveCount(0);

    await page.locator("#edit-agent-runtime").click();
    await page.getByRole("menuitemradio", { name: /^Goose$/ }).click();
    await expect(page.getByTestId("hermes-profile-field")).toHaveCount(0);
    await waitForAnimations(page);
    await page.locator("#edit-agent-runtime").click();
    await waitForAnimations(page);
    await page.getByRole("menuitemradio", { name: /Hermes Agent/ }).click();
    await expect(page.locator("#edit-agent-hermes-profile")).toHaveValue("");
  });

  test("edit: goose does not show Hermes profile fields", async ({ page }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: GOOSE_AGENT_PUBKEY,
          name: "Tyler Agent",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "goose",
        },
      ],
    });

    await openEditForAgent(page, "Tyler Agent");
    await waitForAnimations(page);

    await expect(page.getByTestId("hermes-profile-field")).toHaveCount(0);
    await expect(page.getByTestId("profile-owned-model-row")).toHaveCount(0);
  });

  test("edit: profile-bound Hermes cannot be changed to public access", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: HERMES_AGENT_PUBKEY,
          name: "Hermes Scout",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "hermes",
          hermesProfile: "scout",
          respondTo: "owner-only",
        },
      ],
    });

    await openEditForAgent(page, "Hermes Scout");
    await page.locator("#agent-respond-to").click();
    await page.getByRole("menuitemradio", { name: "Anyone" }).click();

    const dialog = page.getByTestId("edit-agent-dialog");
    const error = dialog.getByTestId("hermes-trusted-boundary-error");
    await expect(error).toContainText(/owner-only/i);
    await expect(error).toContainText(/choose.*Only me|change.*access/i);
    await expect(page.getByTestId("edit-agent-dialog-submit")).toBeDisabled();
  });

  test("edit: duplicate profile bind surfaces occupancy error before save", async ({
    page,
  }) => {
    await installMockBridge(page, {
      hermesProfiles: ["scout", "twin"],
      managedAgents: [
        {
          pubkey: HERMES_AGENT_PUBKEY,
          name: "Hermes Scout",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "hermes",
          hermesProfile: "scout",
        },
        {
          pubkey: GOOSE_AGENT_PUBKEY,
          name: "Hermes Twin",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "hermes",
          hermesProfile: "twin",
        },
      ],
    });

    await openEditForAgent(page, "Hermes Twin");
    await waitForAnimations(page);

    await page.locator("#edit-agent-hermes-profile").fill("scout");
    await expect(page.getByTestId("hermes-profile-error")).toContainText(
      /already bound/i,
    );
    await expect(page.getByTestId("edit-agent-dialog-submit")).toBeDisabled();
  });

  test("delete: bound Hermes agent shows keep/archive choice defaulting to keep", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: HERMES_AGENT_PUBKEY,
          name: "Hermes Scout",
          status: "stopped",
          channelNames: ["agents"],
          runtime: "hermes",
          hermesProfile: "scout",
          respondTo: "owner-only",
        },
      ],
    });

    await page.goto("/");
    await page.getByTestId("open-agents-view").click();
    const agentButton = page.getByRole("button", {
      name: "Hermes Scout agent profile",
    });
    await expect(agentButton).toBeVisible({ timeout: 10_000 });
    await agentButton.click();
    await expect(page.getByTestId("user-profile-panel")).toBeVisible({
      timeout: 10_000,
    });

    await page.getByTestId("user-profile-settings-menu-trigger").click();
    await page.getByRole("menuitem", { name: /Delete agent/i }).click();
    await waitForAnimations(page);

    const dialog = page.getByTestId("agent-delete-confirm-dialog");
    await expect(dialog).toBeVisible();
    await expect(
      page.getByTestId("hermes-profile-offboard-choice"),
    ).toBeVisible();
    await expect(
      page.getByTestId("hermes-profile-offboard-keep"),
    ).toBeChecked();
    await expect(
      page.getByTestId("hermes-profile-offboard-archive"),
    ).not.toBeChecked();
  });
});
