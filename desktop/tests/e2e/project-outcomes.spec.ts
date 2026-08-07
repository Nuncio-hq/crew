import { expect, test } from "@playwright/test";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const PROJECT_CHANNEL_ID = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const THREAD_ID = "project-outcome-thread";

async function injectActiveProjectThread(
  page: Parameters<typeof installMockBridge>[0],
) {
  await page.evaluate(
    ({ agentPubkey, channelId, conversationId }) => {
      (
        window as Window & {
          __BUZZ_E2E_INJECT_OBSERVER_EVENTS__?: (input: {
            agentPubkey: string;
            events: Array<{
              seq: number;
              timestamp: string;
              kind: string;
              agentIndex: number | null;
              channelId: string | null;
              conversationId: string | null;
              sessionId: string | null;
              turnId: string | null;
              payload: unknown;
            }>;
          }) => void;
        }
      ).__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
        agentPubkey,
        events: [
          {
            seq: 1,
            timestamp: new Date().toISOString(),
            kind: "turn_started",
            agentIndex: 0,
            channelId,
            conversationId,
            sessionId: "project-outcome-session",
            turnId: "project-outcome-turn",
            payload: null,
          },
        ],
      });
    },
    {
      agentPubkey: TEST_IDENTITIES.alice.pubkey,
      channelId: PROJECT_CHANNEL_ID,
      conversationId: THREAD_ID,
    },
  );
}

test("project outcomes stay page-local while opening an in-place thread panel", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "buzz-feature-overrides-v1",
      JSON.stringify({ projects: true }),
    );
  });
  await installMockBridge(page);
  await page.setViewportSize({ width: 1280, height: 720 });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.evaluate(
    ({ channelName, id, pubkey }) => {
      window.__BUZZ_E2E_SEED_MOCK_MESSAGE__?.({
        channelName,
        content: "Project thread root",
        id,
        pubkey,
      });
    },
    {
      channelName: "general",
      id: THREAD_ID,
      pubkey: TEST_IDENTITIES.alice.pubkey,
    },
  );
  await page.getByTestId("open-projects-view").click();
  await expect(page.getByTestId("projects-outcome-landing")).toBeVisible();

  const landing = page.getByTestId("projects-outcome-landing");
  const cards = landing.locator('[data-testid^="project-outcome-card-"]');
  await expect(cards.first()).toBeVisible();
  await waitForAnimations(page);
  await cards.first().screenshot({
    path: "test-results/project-outcomes/01-landing-card.png",
  });

  await cards.first().getByRole("button").click();
  await expect(page.getByTestId("project-outcome-page")).toBeVisible();
  await injectActiveProjectThread(page);
  const projectUrl = page.url();
  expect(projectUrl).toContain("/projects/");
  await waitForAnimations(page);
  await page.getByTestId("project-outcome-page").screenshot({
    path: "test-results/project-outcomes/02-project-page.png",
  });

  const inFlight = page.getByTestId(`project-in-flight-${THREAD_ID}`);
  await expect(inFlight).toBeVisible();
  await inFlight.click();
  await expect(page.getByTestId("project-in-flight-panel")).toBeVisible();
  await expect(page.getByTestId("message-thread-panel")).toBeVisible();
  await expect(page.getByTestId("message-thread-panel")).toContainText(
    "Project thread root",
  );
  await expect(page.getByTestId("message-thread-panel")).toHaveCount(1);
  expect(page.url()).toBe(projectUrl);
  expect(page.url()).not.toContain("/channels/");
  await waitForAnimations(page);
  await page.getByTestId("project-in-flight-panel").screenshot({
    path: "test-results/project-outcomes/03-in-place-thread-panel.png",
  });

  const plumbing = page.getByTestId("project-plumbing");
  await expect(plumbing).not.toHaveAttribute("open", "");
  await plumbing.locator("summary").click();
  await expect(plumbing).toHaveAttribute("open", "");
  await waitForAnimations(page);
  await plumbing.screenshot({
    path: "test-results/project-outcomes/04-plumbing-expanded.png",
  });
});
