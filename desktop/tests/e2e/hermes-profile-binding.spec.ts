/**
 * Hermes profile binding UI (Phase 02B / feature 0001).
 *
 * Pins capability-driven visibility: profileArg runtimes show the binding
 * field + profile-owned model row; goose does not. Invalid names and
 * "default" are blocked client-side. Model is never an editable control.
 */
import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const HERMES_AGENT_PUBKEY = TEST_IDENTITIES.alice.pubkey;
const GOOSE_AGENT_PUBKEY = TEST_IDENTITIES.tyler.pubkey;

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

  test("create: goose hides Hermes profile field", async ({ page }) => {
    await installMockBridge(page);
    const dialog = await openCreateCustomize(page);

    await pickRuntime(page, dialog, /^Goose$/);
    await waitForAnimations(page);

    await expect(page.getByTestId("hermes-profile-field")).toHaveCount(0);
    await expect(page.getByTestId("profile-owned-model-row")).toHaveCount(0);
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

  test("edit: Hermes agent shows binding + profile-owned model; goose does not", async ({
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

    await page.keyboard.press("Escape");
    await expect(page.getByTestId("edit-agent-dialog")).not.toBeVisible();

    await openEditForAgent(page, "Tyler Agent");
    await waitForAnimations(page);

    await expect(page.getByTestId("hermes-profile-field")).toHaveCount(0);
    await expect(page.getByTestId("profile-owned-model-row")).toHaveCount(0);
  });

  test("edit: duplicate profile bind surfaces server error inline", async ({
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
    await page.getByTestId("edit-agent-dialog-submit").click();

    await expect(page.getByTestId("edit-agent-save-error")).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByTestId("edit-agent-save-error")).toContainText(
      /already bound/i,
    );
    await expect(page.getByTestId("edit-agent-dialog")).toBeVisible();
  });
});
