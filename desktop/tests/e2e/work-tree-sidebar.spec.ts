import { expect, test, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

import { waitForAnimations } from "../helpers/animations";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { FEATURE_OVERRIDES_STORAGE_KEY } from "../helpers/features";

const SHOTS = "test-results/work-tree-sidebar";
const ENGINEERING_ID = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const ROOT_A = "1".repeat(64);
const REQUEST_ID = "4".repeat(64);
const LIVE_REPLY_ID = "7".repeat(64);
const OWNER = "deadbeef".repeat(8);
const HERMES = TEST_IDENTITIES.alice.pubkey;
const CODEX = TEST_IDENTITIES.bob.pubkey;
const SIDEBAR_CLIP = { x: 0, y: 0, width: 280, height: 720 };

test.use({ video: "on", viewport: { width: 1280, height: 720 } });
test.describe.configure({ timeout: 90_000 });

async function seedGlowmaxProject(page: Page) {
  await page.addInitScript(
    ({ channelId, featureKey, pubkey }) => {
      window.localStorage.setItem(
        featureKey,
        JSON.stringify({ projects: true }),
      );
      const win = window as typeof window & {
        __BUZZ_E2E_EXTRA_PROJECT_EVENTS__?: Array<{
          id: string;
          kind: number;
          pubkey: string;
          created_at: number;
          content: string;
          tags: string[][];
          sig: string;
        }>;
      };
      win.__BUZZ_E2E_EXTRA_PROJECT_EVENTS__ = [
        {
          id: "aa".repeat(32),
          kind: 30617,
          pubkey,
          created_at: Math.floor(Date.now() / 1000),
          content: "Glowmax",
          tags: [
            ["d", "glowmax"],
            ["name", "glowmax"],
            ["buzz-channel", channelId],
            ["clone", "https://github.com/Nuncio-hq/glowmax.git"],
          ],
          sig: "mocksig".repeat(20).slice(0, 128),
        },
      ];
    },
    {
      channelId: ENGINEERING_ID,
      featureKey: FEATURE_OVERRIDES_STORAGE_KEY,
      pubkey: OWNER,
    },
  );
}

async function waitForLive(page: Page, channelName: string) {
  await expect
    .poll(async () =>
      page.evaluate(
        (name) =>
          window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
            channelName: name,
          }) ?? false,
        channelName,
      ),
    )
    .toBe(true);
}

async function seedWorkThread(page: Page, createdAt: number) {
  await page.evaluate(
    ({ channelId, codex, createdAt: at, hermes, owner, requestId, rootId }) => {
      const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
      const emitInput = window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__;
      if (!emit || !emitInput) throw new Error("Mock emit helpers missing.");
      emit({
        channelName: "engineering",
        content: "Fix paywall crash",
        createdAt: at,
        id: rootId,
        mentionPubkeys: [hermes, codex],
        pubkey: owner,
      });
      emitInput({
        channelName: "engineering",
        content: JSON.stringify({
          channel_id: channelId,
          engine: "codex",
          message: "Ship the paywall fix?",
          questions: [
            {
              header: "Choice",
              id: "q0",
              options: [
                { description: "", label: "Yes", value: "yes" },
                { description: "", label: "No", value: "no" },
              ],
              question: "Merge?",
            },
          ],
          request_id: requestId,
          session_id: "work-tree-session",
          turn_id: "work-tree-turn",
        }),
        pubkey: hermes,
        requestId,
        rootEventId: rootId,
      });
    },
    {
      channelId: ENGINEERING_ID,
      codex: CODEX,
      createdAt,
      hermes: HERMES,
      owner: OWNER,
      requestId: REQUEST_ID,
      rootId: ROOT_A,
    },
  );
}

async function injectWorkspace(page: Page) {
  await page.evaluate(
    ({ agentPubkey, channelId }) => {
      const now = new Date().toISOString();
      window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
        agentPubkey,
        events: [
          {
            agentIndex: 0,
            channelId,
            kind: "thread_workspace_ready",
            payload: {
              baseRevision: "abc123",
              baseSource: "remote",
              branch: "fix-paywall",
              commitsBehindRemote: 0,
              remoteDefaultBranch: "main",
              repositoryPath: "/tmp/crew",
              rootEventId: "1".repeat(64),
              worktreeName: "crew-aaaaaaaaaaaa",
              worktreePath: "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa",
            },
            seq: 1,
            sessionId: "wt-session",
            timestamp: now,
            turnId: "wt-turn",
          },
        ],
      });
    },
    { agentPubkey: HERMES, channelId: ENGINEERING_ID },
  );
}

test.describe("work-tree sidebar (#203)", () => {
  test("folder is office, thread is the channel session, needs-you deep-links", async ({
    page,
  }) => {
    await seedGlowmaxProject(page);
    await installMockBridge(page, {
      managedAgents: [
        {
          pubkey: HERMES,
          name: "Hermes",
          status: "running",
          channelNames: ["engineering"],
        },
        {
          pubkey: CODEX,
          name: "Codex",
          status: "running",
          channelNames: ["engineering"],
        },
      ],
    });
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await expect(
      page.getByTestId("work-tree-folder-engineering"),
    ).toBeVisible();
    await expect(
      page.getByTestId("stream-list").getByTestId("channel-general"),
    ).toBeVisible();
    await expect(
      page.getByTestId("stream-list").getByTestId("channel-random"),
    ).toBeVisible();
    await expect(
      page.getByTestId("stream-list").getByTestId("channel-engineering"),
    ).toHaveCount(0);
    await expect(page.getByTestId("dm-list")).toBeVisible();
    await expect(page.getByTestId("needs-you-section")).toHaveCount(0);
    await expect(page.getByTestId("work-tree-folder-badge-live")).toHaveCount(
      0,
    );
    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/01-all-quiet.png`,
      clip: SIDEBAR_CLIP,
    });

    await page.getByTestId("channel-engineering").click();
    await expect(page.getByTestId("chat-title")).toContainText("engineering");
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await waitForLive(page, "engineering");
    await expect
      .poll(async () =>
        page.evaluate(
          () =>
            window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
              channelName: "engineering",
              kind: 46040,
            }) ?? false,
        ),
      )
      .toBe(true);
    await page.waitForFunction(
      () =>
        typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function" &&
        typeof window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__ === "function",
    );

    await seedWorkThread(page, Math.floor(Date.now() / 1000) - 60);
    await injectWorkspace(page);

    const threadRow = page.getByTestId(`work-thread-row-${ROOT_A}`);
    await expect(threadRow).toBeVisible();
    await expect(page.getByTestId("needs-you-section")).toBeVisible();
    await expect(
      page.getByTestId("work-tree-folder-badge-needs-you"),
    ).toBeVisible();
    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/02-expanded-folder.png`,
      clip: SIDEBAR_CLIP,
    });

    await page.getByTestId("work-tree-disclosure-engineering").click();
    await expect(threadRow).toHaveCount(0);
    await expect(
      page.getByTestId("work-tree-disclosure-engineering"),
    ).toHaveAttribute("aria-expanded", "false");
    await expect(
      page.getByTestId("work-tree-folder-badge-needs-you"),
    ).toBeVisible();

    await page.evaluate(
      ({ content, id, parentEventId, pubkey }) => {
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          channelName: "engineering",
          content,
          createdAt: Math.floor(Date.now() / 1000) + 2,
          id,
          parentEventId,
          pubkey,
        });
      },
      {
        content: "Live bump on the paywall thread",
        id: LIVE_REPLY_ID,
        parentEventId: ROOT_A,
        pubkey: HERMES,
      },
    );
    await expect(
      page.getByTestId("work-tree-disclosure-engineering"),
    ).toHaveAttribute("aria-expanded", "false");
    await expect(threadRow).toHaveCount(0);
    await expect(
      page.getByTestId("work-tree-folder-badge-needs-you"),
    ).toBeVisible();
    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/03-collapsed-live.png`,
      clip: SIDEBAR_CLIP,
    });

    await page.getByTestId("needs-you-header").click();
    await expect(page.getByTestId("needs-you-panel")).toBeVisible();
    await expect(
      page.getByTestId(`needs-you-item-${REQUEST_ID}`),
    ).toBeVisible();
    await waitForAnimations(page);
    await page.screenshot({
      path: `${SHOTS}/04-needs-you-panel.png`,
      clip: SIDEBAR_CLIP,
    });

    await page.getByTestId(`needs-you-item-${REQUEST_ID}`).click();
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await expect(page.getByTestId("workbench-screen")).toHaveCount(0);

    await page.getByTestId("work-tree-disclosure-engineering").click();
    await expect(threadRow).toBeVisible();
    await threadRow.click();
    await expect(page.getByTestId("message-thread-panel")).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await expect(page.getByTestId("workbench-screen")).toHaveCount(0);

    await page.getByTestId("work-tree-disclosure-engineering").focus();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(new RegExp(`/channels/${ENGINEERING_ID}`));
    await page.getByTestId("work-tree-disclosure-engineering").focus();
    await page.keyboard.press("ArrowLeft");
    await expect(
      page.getByTestId("work-tree-disclosure-engineering"),
    ).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("ArrowRight");
    await expect(
      page.getByTestId("work-tree-disclosure-engineering"),
    ).toHaveAttribute("aria-expanded", "true");
    await expect(threadRow).toBeVisible();

    const files = (await readdir(SHOTS)).filter((name) =>
      name.endsWith(".png"),
    );
    const hashes = new Set<string>();
    for (const file of files) {
      const bytes = await readFile(path.join(SHOTS, file));
      hashes.add(createHash("sha256").update(bytes).digest("hex"));
    }
    expect(hashes.size, "sidebar screenshots must be hash-distinct").toBe(
      files.length,
    );
  });
});
