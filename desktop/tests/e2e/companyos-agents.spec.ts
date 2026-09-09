import { expect, test, type Page } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

// Mock UI evidence only. Native process termination and durable canvas cleanup
// must be verified separately against the isolated #348 staging deployment.
const PUBKEY = TEST_IDENTITIES.charlie.pubkey;
const NAME = "CompanyOS disposable agent";
const SHOTS = "test-results/companyos-agents";

type AgentTestWindow = Window & {
  __353FailInventory?: boolean;
  __353Deletes?: string[];
  __TAURI_INTERNALS__: {
    invoke: (
      command: string,
      payload: unknown,
      options?: unknown,
    ) => Promise<unknown>;
  };
};

async function openDirectory(page: Page) {
  await installMockBridge(page, {
    managedAgents: [
      { pubkey: PUBKEY, name: NAME, personaId: null, status: "stopped" },
    ],
  });
  await page.goto("/");
  if ((page.viewportSize()?.width ?? 1280) < 768) {
    await page
      .getByRole("button", { name: "Toggle Sidebar", exact: true })
      .click();
  }
  await page.getByTestId("open-agents-view").click();
  if ((page.viewportSize()?.width ?? 1280) < 768) {
    await page.keyboard.press("Escape");
  }
  await expect(page.getByTestId(`managed-agent-${PUBKEY}`)).toBeVisible();
}

test("inventory failure exposes a working keyboard Retry without losing the directory", async ({
  page,
}) => {
  await openDirectory(page);
  await page.evaluate(async () => {
    const state = window as AgentTestWindow;
    const original = state.__TAURI_INTERNALS__.invoke.bind(
      state.__TAURI_INTERNALS__,
    );
    state.__353FailInventory = true;
    state.__TAURI_INTERNALS__.invoke = (command, payload, options) => {
      if (command === "list_managed_agents" && state.__353FailInventory) {
        return Promise.reject(new Error("Directory fixture unavailable"));
      }
      return original(command, payload, options);
    };
    await window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
      queryKey: ["managed-agents"],
    });
  });
  const retry = page.getByRole("button", { name: "Retry agents", exact: true });
  await expect(retry).toBeVisible();
  await expect(page.getByTestId(`managed-agent-${PUBKEY}`)).toBeVisible();
  await waitForAnimations(page);
  await page
    .getByRole("alert")
    .filter({ hasText: "Directory fixture unavailable" })
    .screenshot({ path: `${SHOTS}/inventory-retry.png` });
  await page.evaluate(() => {
    (window as AgentTestWindow).__353FailInventory = false;
  });
  await retry.focus();
  await page.keyboard.press("Enter");
  await expect(retry).toHaveCount(0);
  await expect(page.getByTestId(`managed-agent-${PUBKEY}`)).toBeVisible();
});

test("instance Delete names its target and keyboard Cancel preserves it at a narrow width", async ({
  page,
}) => {
  await page.setViewportSize({ width: 760, height: 900 });
  await openDirectory(page);
  await page.evaluate(() => {
    const state = window as AgentTestWindow;
    state.__353Deletes = [];
    const original = state.__TAURI_INTERNALS__.invoke.bind(
      state.__TAURI_INTERNALS__,
    );
    state.__TAURI_INTERNALS__.invoke = (command, payload, options) => {
      if (command === "delete_managed_agent")
        state.__353Deletes?.push((payload as { pubkey: string }).pubkey);
      return original(command, payload, options);
    };
  });
  await page
    .getByRole("button", { name: `${NAME} agent profile`, exact: true })
    .click();
  const remove = page.getByTestId("user-profile-delete-agent-row");
  await remove.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByTestId("agent-delete-confirm-dialog");
  await expect(dialog).toContainText(NAME);
  await expect(dialog).toContainText("Preserves message history");
  await expect(dialog).toBeVisible();
  await waitForAnimations(page);
  await dialog.screenshot({ path: `${SHOTS}/instance-delete-confirm.png` });
  await dialog.getByRole("button", { name: "Cancel", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect
    .poll(() => page.evaluate(() => (window as AgentTestWindow).__353Deletes))
    .toEqual([]);
  await expect(remove).toBeVisible();
  await expect(page.getByTestId(`managed-agent-${PUBKEY}`)).toBeVisible();
});

test("a native stop failure keeps the instance available for another deletion attempt", async ({
  page,
}) => {
  await openDirectory(page);
  await page.evaluate(() => {
    const state = window as AgentTestWindow;
    const original = state.__TAURI_INTERNALS__.invoke.bind(
      state.__TAURI_INTERNALS__,
    );
    state.__TAURI_INTERNALS__.invoke = (command, payload, options) => {
      if (command === "delete_managed_agent")
        return Promise.reject(new Error("Could not stop disposable process"));
      return original(command, payload, options);
    };
  });
  await page
    .getByRole("button", { name: `${NAME} agent profile`, exact: true })
    .click();
  await page.getByTestId("user-profile-delete-agent-row").click();
  await page.getByTestId("agent-delete-confirm-action").click();
  await expect(
    page.getByText("Could not stop disposable process", { exact: true }),
  ).toBeVisible();
  await expect(page.getByTestId("user-profile-delete-agent-row")).toBeVisible();
  await expect(page.getByTestId(`managed-agent-${PUBKEY}`)).toBeVisible();
});

test("relay-only colleague opens its exact profile and scoped direct conversation without local controls", async ({
  page,
}) => {
  const remoteKey = TEST_IDENTITIES.alice.pubkey;
  await installMockBridge(page, {
    managedAgents: [],
    searchProfiles: [
      {
        pubkey: remoteKey,
        displayName: "Relay colleague",
        ownerPubkey: "deadbeef".repeat(8),
        isAgent: true,
      },
    ],
    relayAgents: [
      {
        pubkey: remoteKey,
        name: "Relay colleague",
        ownerPubkey: "deadbeef".repeat(8),
        channelNames: ["agents"],
      },
    ],
  });
  await page.goto("/");
  await page.getByTestId("open-agents-view").click();
  const card = page.getByTestId(`relay-only-agent-${remoteKey}`);
  await expect(card).toBeVisible();
  await expect(card.getByRole("button")).toHaveCount(1);
  await card.getByRole("button").click();
  const panel = page.getByTestId("user-profile-panel");
  await expect(panel.getByTestId("user-profile-message")).toBeVisible();
  await expect(panel.getByTestId("user-profile-delete-agent-row")).toHaveCount(
    0,
  );
  await expect(panel.getByTestId("user-profile-edit-agent")).toHaveCount(0);
  await page.evaluate(() => {
    const state = window as AgentTestWindow & { __353Dm?: unknown };
    const original = state.__TAURI_INTERNALS__.invoke.bind(
      state.__TAURI_INTERNALS__,
    );
    state.__TAURI_INTERNALS__.invoke = (command, payload, options) => {
      if (command === "open_dm") state.__353Dm = payload;
      return original(command, payload, options);
    };
  });
  await panel.getByTestId("user-profile-message").click();
  await expect(page).toHaveURL(/\/channels\//);
  const captured = await page.evaluate(() => {
    const state = window as AgentTestWindow & {
      __353Dm?: {
        pubkeys: string[];
        expectedRelayUrl: string;
        expectedSignerPubkey: string;
      };
    };
    const communities = JSON.parse(
      localStorage.getItem("buzz-communities") ?? "[]",
    ) as { id: string; relayUrl: string }[];
    return {
      payload: state.__353Dm,
      relay: communities.find(
        (community) =>
          community.id === localStorage.getItem("buzz-active-community-id"),
      )?.relayUrl,
    };
  });
  expect(captured.payload).toEqual({
    pubkeys: [remoteKey],
    expectedRelayUrl: captured.relay,
    expectedSignerPubkey: "deadbeef".repeat(8),
  });
});

test("scoped Start retains the selected card's pending affordance", async ({
  page,
}) => {
  await openDirectory(page);
  await page.evaluate(() => {
    const state = window as AgentTestWindow & {
      __353ReleaseStart?: () => void;
    };
    const original = state.__TAURI_INTERNALS__.invoke.bind(
      state.__TAURI_INTERNALS__,
    );
    state.__TAURI_INTERNALS__.invoke = (command, payload, options) => {
      if (command === "start_managed_agent")
        return new Promise((resolve, reject) => {
          state.__353ReleaseStart = () => {
            original(command, payload, options).then(resolve, reject);
          };
        });
      return original(command, payload, options);
    };
  });
  const start = page.getByTestId(`agent-runtime-start-${PUBKEY}`);
  await start.click();
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute("aria-label", /Starting/);
  await page.evaluate(() => {
    (
      window as AgentTestWindow & { __353ReleaseStart?: () => void }
    ).__353ReleaseStart?.();
  });
  await expect(
    page.getByTestId(`agent-runtime-active-${PUBKEY}`),
  ).toBeVisible();
});
