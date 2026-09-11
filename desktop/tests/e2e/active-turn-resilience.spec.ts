import { expect, test } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

// Mock agent pubkeys (distinct from the relay agents seeded by default).
const AGENT_PAUL = "aa".repeat(32);
const AGENT_DUNCAN = "bb".repeat(32);

// Mock channel IDs from the e2e bridge.
const CHANNEL_GENERAL = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const CHANNEL_ENGINEERING = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";

// A fixed epoch so the mocked clock is deterministic across runs.
const T0 = new Date("2026-06-18T12:00:00.000Z");

// The target turn is older than REMOVE_AFTER_MS (3 minutes) when the final
// shared gap is applied. A sibling turn is seeded 150s later, so at the final
// assertion max(lastSeenAt) is only 40s stale: past FRAME_GAP_PAUSE_MS (20s)
// but below PRUNE_PAUSE_MAX_MS (3 minutes). Several 5s prune ticks therefore
// run while shouldPausePrune is the only thing keeping the old target alive.
const TARGET_HEAD_START_MS = 150_000;
const FRAME_GAP_MS = 40_000;

type SeedInput = {
  agentPubkey: string;
  channelId: string;
  turnId: string;
};

async function waitForBridge(page: import("@playwright/test").Page) {
  await page.waitForFunction(
    () =>
      typeof (window as Window & { __BUZZ_E2E_SEED_ACTIVE_TURNS__?: unknown })
        .__BUZZ_E2E_SEED_ACTIVE_TURNS__ === "function",
    null,
    { timeout: 10_000 },
  );
}

async function openAgentsView(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await waitForBridge(page);
  await page.getByTestId("open-agents-view").click();
  await expect(page.getByTestId("unified-agents-groups")).toBeVisible({
    timeout: 10_000,
  });
}

async function seedTurns(
  page: import("@playwright/test").Page,
  turns: SeedInput[],
) {
  await page.evaluate((seeds) => {
    const win = window as Window & {
      __BUZZ_E2E_SEED_ACTIVE_TURNS__?: (input: {
        agentPubkey: string;
        channelId: string;
        turnId: string;
      }) => void;
    };
    for (const seed of seeds) win.__BUZZ_E2E_SEED_ACTIVE_TURNS__?.(seed);
  }, turns);
}

async function openAgentProfile(
  page: import("@playwright/test").Page,
  pubkey: string,
) {
  await page.getByTestId(`managed-agent-${pubkey}`).click();
  const panel = page.getByTestId("user-profile-panel");
  await expect(panel).toBeVisible({ timeout: 5_000 });
  return panel;
}

test.describe("active turn profile activity resilience", () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  test("profile activity channels persist through an all-at-once liveness gap", async ({
    page,
  }) => {
    // Install the mocked clock BEFORE navigation so the store's Date.now() /
    // setInterval and the activity feed's useNow(15_000) all run on the mocked
    // clock from module init. The target turn then stamps lastSeenAt at T0.
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: AGENT_PAUL,
          name: "Paul",
          status: "running",
          channelNames: ["general", "engineering"],
        },
        {
          pubkey: AGENT_DUNCAN,
          name: "Duncan",
          status: "running",
          channelNames: ["general"],
        },
      ],
    });
    await page.clock.install({ time: T0 });

    await openAgentsView(page);

    // Start the target turn first, then let it age close to the prune bound.
    // Its startedAt and lastSeenAt remain anchored at T0.
    await seedTurns(page, [
      {
        agentPubkey: AGENT_PAUL,
        channelId: CHANNEL_GENERAL,
        turnId: "t-paul-g",
      },
    ]);

    // A later sibling frame keeps the agent's max(lastSeenAt) distinct from
    // the target turn's age. This is the all-at-once gap signature: both turns
    // stop receiving frames only after the sibling has become active.
    await page.clock.fastForward(TARGET_HEAD_START_MS);
    await seedTurns(page, [
      {
        agentPubkey: AGENT_PAUL,
        channelId: CHANNEL_ENGINEERING,
        turnId: "t-paul-e",
      },
      {
        agentPubkey: AGENT_DUNCAN,
        channelId: CHANNEL_GENERAL,
        turnId: "t-duncan-g",
      },
    ]);

    // Simulate the all-at-once relay drop while the Agents view remains
    // mounted. The active-turn subscription is therefore live before the
    // prune ticks run, and the profile opened below observes the store's
    // post-prune state instead of relying on a re-render that may be queued.
    // shouldPausePrune sees the sibling's lastSeenAt only 40s old (gap >20s
    // and <180s) and pauses the prune, so both channels survive. Without that
    // guard, the target channel is removed at the first tick at or after 180s.
    await page.clock.fastForward(FRAME_GAP_MS);

    const paulPanel = await openAgentProfile(page, AGENT_PAUL);
    const liveActivity = paulPanel.getByTestId(
      `user-profile-live-activity-${AGENT_PAUL}`,
    );
    await expect(liveActivity).toBeVisible({ timeout: 5_000 });
    await expect(liveActivity).toContainText("Latest Activity");
    // The profile activity carousel is the current supported surface for the
    // store-driven working channels in the CompanyOS shell.
    const generalActivity = paulPanel.getByTestId(
      `user-profile-activity-dot-${CHANNEL_GENERAL}`,
    );
    const engineeringActivity = paulPanel.getByTestId(
      `user-profile-activity-dot-${CHANNEL_ENGINEERING}`,
    );
    await expect(generalActivity).toBeVisible();
    await expect(engineeringActivity).toBeVisible();
  });
});
