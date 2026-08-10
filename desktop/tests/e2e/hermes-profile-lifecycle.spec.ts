/**
 * Issue #119 acceptance coverage for named Hermes readiness and profile
 * archive lifecycle surfaces.
 */
import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

const AGENT_PUBKEY = "cc".repeat(32);

async function openAgents(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 1600, height: 1000 });
  await page.goto("/");
  await page.getByTestId("open-agents-view").click();
  await expect(page.getByTestId("unified-agents-groups")).toBeVisible({
    timeout: 10_000,
  });
}

test.describe("Hermes profile lifecycle acceptance", () => {
  test("readiness card renders every named state", async ({ page }) => {
    const states = [
      {
        state: "ready",
        expectedId: "hermes-readiness-ready",
        copy: "Ready",
      },
      {
        state: "missing",
        profile: "scout",
        expectedId: "hermes-readiness-missing",
        copy: "Profile missing",
      },
      {
        state: "broken_config",
        profile: "scout",
        diagnostic: "invalid yaml",
        expectedId: "hermes-readiness-broken-config",
        copy: "Config invalid",
      },
      {
        state: "binary_missing",
        command: "hermes",
        expectedId: "hermes-readiness-binary-missing",
        copy: "Binary missing",
      },
      {
        state: "auth_unknown",
        profile: "scout",
        expectedId: "hermes-readiness-auth-unknown",
        copy: "Auth not verifiable",
      },
    ] as const;

    for (const readiness of states) {
      await installMockBridge(page, {
        managedAgents: [
          {
            pubkey: AGENT_PUBKEY,
            name: "Hermes Readiness Fixture",
            status: "stopped",
            runtime: "hermes",
            hermesProfile: "scout",
            profileReadiness: readiness,
          },
        ],
      });
      await openAgents(page);
      const indicator = page.getByTestId(readiness.expectedId);
      await expect(indicator).toBeVisible();
      await expect(indicator).toContainText(readiness.copy);
      await indicator.screenshot({
        path: `/home/ubuntu/119-evidence/readiness-${readiness.state}.png`,
      });
      if (readiness.state === "auth_unknown") {
        await expect(indicator).not.toHaveClass(/destructive|warning/);
        await expect(indicator).toContainText("Auth not verifiable");
      }
    }
  });

  test("offboarding keeps by default and disables archive while running", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: "Running Hermes",
          status: "running",
          runtime: "hermes",
          hermesProfile: "scout",
        },
      ],
    });
    await openAgents(page);
    await page
      .getByRole("button", { name: "Running Hermes agent profile" })
      .click();
    await page.getByTestId("user-profile-settings-menu-trigger").click();
    await page.getByRole("menuitem", { name: /Delete agent/i }).click();

    const dialog = page.getByTestId("agent-delete-confirm-dialog");
    await expect(dialog).toBeVisible();
    await expect(
      page.getByTestId("hermes-profile-offboard-keep"),
    ).toBeChecked();
    const archive = page.getByTestId("hermes-profile-offboard-archive");
    await expect(archive).not.toBeChecked();
    await expect(archive).toBeDisabled();
    await expect(dialog).toContainText(
      /Stop.*Running Hermes|Stop the running agent/i,
    );
  });

  test("offboarding archive choice shows estimate and optional reason", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: "Stopped Hermes",
          status: "stopped",
          runtime: "hermes",
          hermesProfile: "scout",
        },
      ],
    });
    await openAgents(page);
    await page
      .getByRole("button", { name: "Stopped Hermes agent profile" })
      .click();
    await page.getByTestId("user-profile-settings-menu-trigger").click();
    await page.getByRole("menuitem", { name: /Delete agent/i }).click();
    const dialog = page.getByTestId("agent-delete-confirm-dialog");
    await dialog.getByTestId("hermes-profile-offboard-archive").check();
    await expect(
      page.getByTestId("hermes-profile-archive-estimate"),
    ).toContainText(/Estimated archive:|excluded/i);
    await expect(
      page.getByTestId("hermes-profile-offboard-reason"),
    ).toBeVisible();
    await expect(
      page.getByTestId("hermes-profile-offboard-reason"),
    ).not.toHaveAttribute("required", "");
    await dialog.screenshot({
      path: "/home/ubuntu/119-evidence/offboard-archive-dialog.png",
    });
  });

  test("archive panel exposes manifest facts and exact-name delete gate", async ({
    page,
  }) => {
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PUBKEY,
          name: "Hermes Archive Fixture",
          status: "stopped",
          runtime: "hermes",
          hermesProfile: "scout",
        },
      ],
    });
    await openAgents(page);
    await page.getByTestId("hermes-profile-archives-button").click();
    await expect(
      page.getByTestId("hermes-profile-archives-empty"),
    ).toBeVisible();

    // Drive the deterministic bridge directly to seed an archive for the
    // listing/restore/delete surface, while still exercising the real UI.
    await page.evaluate(async () => {
      const invoke = (
        window as Window & {
          __BUZZ_E2E_INVOKE_MOCK_COMMAND__?: (
            command: string,
            payload?: unknown,
          ) => Promise<unknown>;
        }
      ).__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
      const result = await invoke?.("archive_hermes_profile", {
        profile: "scout",
        reason: "E2E archive",
      });
      if (!result || (result as { status?: string }).status !== "archived") {
        throw new Error(`archive seed failed: ${JSON.stringify(result)}`);
      }
    });
    await page.getByRole("button", { name: /Close/i }).click();
    await page.getByTestId("hermes-profile-archives-button").click();
    const row = page.getByTestId(
      "hermes-profile-archive-row-scout-mock-archive",
    );
    await expect(row).toBeVisible();
    await expect(row).toContainText(/scout|E2E archive|audio_cache/i);
    await row.screenshot({
      path: "/home/ubuntu/119-evidence/archives-list.png",
    });
    await row
      .getByTestId("hermes-profile-archive-restore-scout-mock-archive")
      .click();
    await expect(
      row.getByTestId("hermes-profile-archive-rebind-scout-mock-archive"),
    ).toBeVisible();

    const seedArchive = async (idProfile: string) => {
      await page.evaluate(async (profile) => {
        const invoke = (
          window as Window & {
            __BUZZ_E2E_INVOKE_MOCK_COMMAND__?: (
              command: string,
              payload?: unknown,
            ) => Promise<unknown>;
          }
        ).__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
        await invoke?.("archive_hermes_profile", { profile });
      }, idProfile);
    };
    await seedArchive("scout");
    await page.evaluate(async () => {
      const invoke = (
        window as Window & {
          __BUZZ_E2E_INVOKE_MOCK_COMMAND__?: (
            command: string,
            payload?: unknown,
          ) => Promise<unknown>;
        }
      ).__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
      await invoke?.("create_hermes_profile", { name: "scout" });
    });
    await page.getByRole("button", { name: /Close/i }).click();
    await page.getByTestId("hermes-profile-archives-button").click();
    const collisionRow = page.getByTestId(
      "hermes-profile-archive-row-scout-mock-archive",
    );
    await collisionRow
      .getByTestId("hermes-profile-archive-restore-scout-mock-archive")
      .click();
    await expect(
      page.getByTestId("hermes-profile-archives-error"),
    ).toContainText("profile already exists");
    await row
      .getByTestId("hermes-profile-archive-delete-scout-mock-archive")
      .click();
    const input = page.getByTestId(
      "hermes-profile-archive-confirm-input-scout-mock-archive",
    );
    await expect(
      page.getByTestId(
        "hermes-profile-archive-confirm-delete-scout-mock-archive",
      ),
    ).toBeDisabled();
    await input.fill("wrong");
    await expect(
      page.getByTestId(
        "hermes-profile-archive-confirm-delete-scout-mock-archive",
      ),
    ).toBeDisabled();
    await input.fill("scout");
    await expect(
      page.getByTestId(
        "hermes-profile-archive-confirm-delete-scout-mock-archive",
      ),
    ).toBeEnabled();
    await page
      .getByTestId("hermes-profile-archive-confirm-input-scout-mock-archive")
      .screenshot({
        path: "/home/ubuntu/119-evidence/permanent-delete-confirmation.png",
      });
  });
});
