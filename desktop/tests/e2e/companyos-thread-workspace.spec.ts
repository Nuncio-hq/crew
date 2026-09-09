import { expect, test, type Page } from "@playwright/test";
import { THREAD_FOCUS_SLIVER_WIDTH_PX } from "../../src/features/channels/lib/threadFocusLayout";
import { deriveAgentConversationId } from "../../src/features/agents/conversationId";
import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { waitForAnimations } from "../helpers/animations";

const CHANNEL = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";
const ROOT = "a".repeat(64);
const OTHER_ROOT = "b".repeat(64);
const DEV = TEST_IDENTITIES.alice.pubkey;
const SCOUT = TEST_IDENTITIES.bob.pubkey;
const ACTIVATION_COMMANDS = [
  "browser_open",
  "sim_ensure_device",
  "sim_boot",
  "start_dev_server",
  "terminal_attach",
  "prepare_project_thread_workspace",
];

async function setup(page: Page, mode: "focus" | "split" = "focus") {
  await page.addInitScript(
    (viewMode) =>
      localStorage.setItem("buzz.channels.threadViewMode", viewMode),
    mode,
  );
  await installMockBridge(page, {
    managedAgents: [
      {
        pubkey: DEV,
        name: "Workspace Dev",
        status: "stopped",
        channelNames: ["general"],
      },
      {
        pubkey: SCOUT,
        name: "Workspace Scout",
        status: "stopped",
        channelNames: ["general"],
      },
    ],
  });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await page.waitForFunction(
    () =>
      window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
        channelName: "general",
      }) === true,
  );
  await page.evaluate(
    ({ root, otherRoot, agents }) => {
      for (const [id, content] of [
        [root, "Workspace first thread"],
        [otherRoot, "Workspace second thread"],
      ]) {
        window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
          id,
          channelName: "general",
          content,
          mentionPubkeys: agents,
        });
      }
    },
    { root: ROOT, otherRoot: OTHER_ROOT, agents: [DEV, SCOUT] },
  );
}

async function openThread(
  page: Page,
  root: string,
  mode: "focus" | "split" | "standalone" = "focus",
) {
  const row = page.locator(`[data-message-id="${root}"]`).first();
  await row.hover();
  await row.getByRole("button", { name: "Reply", exact: true }).click();
  await expect(
    page.getByTestId(
      mode === "standalone"
        ? "thread-surface"
        : mode === "focus"
          ? "focus-thread-drawer"
          : "message-thread-panel",
    ),
  ).toBeVisible();
}

for (const mode of ["split", "focus"] as const) {
  test(`Tools opens from ${mode} thread and remains open after focus transition`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1800, height: 1000 });
    await setup(page, mode);
    await openThread(page, ROOT, mode);
    await expect(page.getByTestId("channel-tool-pane")).toHaveCount(0);
    await page.getByRole("button", { name: "Open thread tools" }).click();
    await expect(page.getByTestId("focus-thread-drawer")).toBeVisible();
    await waitForAnimations(page);
    await expect(page.getByTestId("channel-tool-pane")).toHaveCount(1);
    await expect(
      page.getByRole("tab", { name: "Context", exact: true }),
    ).toHaveAttribute("aria-selected", "true");
    expect(await nativeActivations(page)).toEqual([]);
  });
}

async function injectWork(
  page: Page,
  agentPubkey: string,
  label: string,
  completed = false,
) {
  await page.evaluate(
    ({ agent, channel, conversationId, text, done }) => {
      const timestamp = new Date().toISOString();
      const base = {
        agentIndex: 0,
        channelId: channel,
        conversationId,
        sessionId: `session-${agent}`,
        turnId: `turn-${agent}`,
        timestamp,
        startedAt: timestamp,
      };
      window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
        agentPubkey: agent,
        events: [
          { ...base, seq: 1, kind: "turn_started", payload: {} },
          {
            ...base,
            seq: 2,
            kind: "acp_read",
            payload: {
              method: "session/update",
              params: {
                sessionId: base.sessionId,
                update: {
                  sessionUpdate: "plan",
                  entries: [
                    {
                      content: `${text} plan`,
                      status: done ? "completed" : "in_progress",
                    },
                  ],
                },
              },
            },
          },
          {
            ...base,
            seq: 3,
            kind: "acp_read",
            payload: {
              method: "session/update",
              params: {
                sessionId: base.sessionId,
                update: {
                  sessionUpdate: "agent_message_chunk",
                  content: {
                    type: "text",
                    text: `${text} retained transcript`,
                  },
                },
              },
            },
          },
          ...(done
            ? [
                {
                  ...base,
                  seq: 4,
                  kind: "turn_completed" as const,
                  payload: {},
                },
              ]
            : []),
        ],
      });
    },
    {
      agent: agentPubkey,
      channel: CHANNEL,
      conversationId: deriveAgentConversationId(CHANNEL, ROOT),
      text: label,
      done: completed,
    },
  );
}

async function nativeActivations(page: Page) {
  return page.evaluate(
    (commands) =>
      (window.__BUZZ_E2E_COMMANDS__ ?? []).filter((command) =>
        commands.includes(command),
      ),
    ACTIVATION_COMMANDS,
  );
}

test("mock thread workspace has separate agent plans, retained activity and inert tool selection", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1000 });
  await setup(page);
  await openThread(page, ROOT);
  await expect(page.getByTestId("channel-tool-pane")).toHaveCount(0);
  await page.getByRole("button", { name: "Open thread tools" }).click();
  await expect(
    page.getByRole("tab", { name: "Context", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("region", { name: "Thread context" }),
  ).toContainText("Recap: Off");
  await injectWork(page, DEV, "Dev", true);
  await injectWork(page, SCOUT, "Scout");
  await page.getByRole("tab", { name: "Agent plans", exact: true }).click();
  await expect(page.getByTestId(`declared-plan-card-${DEV}`)).toContainText(
    "Dev plan",
  );
  await expect(page.getByTestId(`declared-plan-card-${SCOUT}`)).toContainText(
    "Scout plan",
  );
  await expect(page.getByTestId("declared-plans-rail")).toHaveCount(1);
  await page.getByRole("tab", { name: "Activity", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Activity agent" })
    .selectOption(DEV);
  await expect(page.getByTestId("thread-agent-transcript")).toContainText(
    "Dev retained transcript",
  );
  await page
    .getByRole("combobox", { name: "Activity agent" })
    .selectOption(SCOUT);
  await expect(page.getByTestId("thread-agent-transcript")).toContainText(
    "Scout retained transcript",
  );
  for (const name of ["Browser", "Sim", "Context"])
    await page.getByRole("tab", { name, exact: true }).click();
  expect(await nativeActivations(page)).toEqual([]);
  await page.getByRole("tab", { name: "Agent plans", exact: true }).click();
  await page.getByRole("button", { name: "Close Tools", exact: true }).click();
  await expect(page.getByTestId("channel-tool-pane")).toHaveCount(0);
  await page
    .getByTestId("focus-thread-drawer-scrim")
    .click({ position: { x: THREAD_FOCUS_SLIVER_WIDTH_PX / 2, y: 20 } });
  await openThread(page, OTHER_ROOT);
  await expect(page.getByTestId("channel-tool-pane")).toHaveCount(0);
  await page.getByRole("button", { name: "Open thread tools" }).click();
  await expect(
    page.getByRole("tab", { name: "Context", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await page.getByRole("button", { name: "Close Tools", exact: true }).click();
  await page
    .getByTestId("focus-thread-drawer-scrim")
    .click({ position: { x: THREAD_FOCUS_SLIVER_WIDTH_PX / 2, y: 20 } });
  await openThread(page, ROOT);
  await expect(page.getByTestId("channel-tool-pane")).toHaveCount(0);
  await page.getByRole("button", { name: "Open thread tools" }).click();
  await expect(
    page.getByRole("tab", { name: "Agent plans", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  expect(await nativeActivations(page)).toEqual([]);
  await waitForAnimations(page);
  await page.getByTestId("channel-tool-pane").screenshot({
    path: "test-results/companyos-thread-workspace/mock-agent-plans.png",
  });
});

test("mock narrow tools own Escape and preserve the conversation draft", async ({
  page,
}) => {
  await page.setViewportSize({ width: 800, height: 720 });
  await setup(page);
  await openThread(page, ROOT, "standalone");
  const composer = page
    .getByTestId("thread-composer-overlay")
    .locator('[contenteditable="true"]')
    .first();
  await composer.fill("Keep this unsent draft");
  const opener = page.getByRole("button", { name: "Open thread tools" });
  await opener.click();
  const modal = page.getByRole("dialog", { name: "Thread tools" });
  await expect(modal).toBeVisible();
  await expect(page.getByTestId("thread-surface")).toHaveCount(1);
  await expect(composer).toHaveText("Keep this unsent draft");
  await page.getByRole("tab", { name: "Browser", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Activate Browser", exact: true }),
  ).toBeVisible();
  expect(await nativeActivations(page)).toEqual([]);
  await page.keyboard.press("Escape");
  await expect(modal).toHaveCount(0);
  await expect(page.getByTestId("thread-surface")).toBeVisible();
  await expect(opener).toBeFocused();
  await expect(composer).toHaveText("Keep this unsent draft");
  await opener.click();
  await expect(
    page.getByRole("tab", { name: "Browser", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  expect(await nativeActivations(page)).toEqual([]);
  await waitForAnimations(page);
  await modal.screenshot({
    path: "test-results/companyos-thread-workspace/mock-narrow-tools.png",
  });
});

// IPC-only fixture: the real Activity components, selection gate, and result
// dispatcher execute normally. This is explicitly not native transport proof.
async function installControlFixture(page: Page) {
  await page.evaluate(() => {
    type ControlInput = {
      agentPubkey: string;
      expectedScope: { scope: { owner: string; community: string } };
      payload: {
        channelId: string;
        conversationId: string;
        turnId: string;
        requestId: string;
        type: string;
      };
    };
    const w = window as typeof window & {
      __CREW354_CONTROLS__: {
        mode: "accepted" | "unknown";
        answerFailure: boolean;
        controls: ControlInput[];
        answers: unknown[];
      };
      __TAURI_INTERNALS__: {
        invoke: (
          command: string,
          payload: unknown,
          options: unknown,
        ) => Promise<unknown>;
      };
    };
    w.__CREW354_CONTROLS__ = {
      mode: "accepted",
      answerFailure: false,
      controls: [],
      answers: [],
    };
    const original = w.__TAURI_INTERNALS__.invoke.bind(w.__TAURI_INTERNALS__);
    w.__TAURI_INTERNALS__.invoke = async (command, payload, options) => {
      const fixture = w.__CREW354_CONTROLS__;
      if (command === "owner_operation_scope") {
        const communities = JSON.parse(
          localStorage.getItem("buzz-communities") ?? "[]",
        ) as Array<{ id: string; pubkey: string; relayUrl: string }>;
        const active = communities.find(
          (entry) =>
            entry.id === localStorage.getItem("buzz-active-community-id"),
        );
        if (!active) throw new Error("Missing fixture community");
        return {
          scope: {
            owner: active.pubkey,
            community: new URL(active.relayUrl.replace(/^ws/, "http")).origin,
          },
          workspace_generation: 1,
          identity_generation: 1,
        };
      }
      if (command === "send_scoped_observer_control") {
        const input = payload as ControlInput;
        fixture.controls.push(input);
        if (fixture.mode === "unknown")
          return { status: "unknown", message: "Mock response lost" };
        window.__BUZZ_E2E_INJECT_OBSERVER_EVENTS__?.({
          agentPubkey: input.agentPubkey,
          events: [
            {
              agentIndex: 0,
              seq: 100 + fixture.controls.length,
              kind: "control_result",
              sessionId: `session-${input.agentPubkey}`,
              timestamp: new Date().toISOString(),
              channelId: input.payload.channelId,
              conversationId: input.payload.conversationId,
              turnId: input.payload.turnId,
              payload: { ...input.payload, status: "sent" },
            },
          ],
        });
        return { status: "accepted", event_id: "c".repeat(64) };
      }
      if (command === "send_channel_user_input_answer") {
        fixture.answers.push(payload);
        if (fixture.answerFailure)
          throw new Error("Mock answer publication failed");
      }
      return original(command, payload, options);
    };
  });
}

async function controlFixtureState(page: Page) {
  return page.evaluate(
    () =>
      (
        window as typeof window & {
          __CREW354_CONTROLS__: {
            controls: Array<{
              agentPubkey: string;
              expectedScope: { scope: { owner: string; community: string } };
              payload: {
                channelId: string;
                conversationId: string;
                turnId: string;
                requestId: string;
              };
            }>;
            answers: unknown[];
          };
        }
      ).__CREW354_CONTROLS__,
  );
}

async function openActivity(page: Page) {
  await openThread(page, ROOT);
  await page.getByRole("button", { name: "Open thread tools" }).click();
  await page.getByRole("tab", { name: "Activity", exact: true }).click();
}

async function chooseDevRun(page: Page) {
  await page
    .getByRole("combobox", { name: "Activity agent" })
    .selectOption(DEV);
  const picker = page.getByRole("combobox", { name: "Activity live run" });
  await expect(picker).toBeEnabled();
  await picker.selectOption(JSON.stringify([`session-${DEV}`, `turn-${DEV}`]));
}

test("mock Activity Stop needs explicit live selection and preserves historical transcript", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1000 });
  await setup(page);
  await installControlFixture(page);
  await injectWork(page, DEV, "Dev");
  await injectWork(page, SCOUT, "Scout", true);
  await openActivity(page);
  await expect(
    page.getByRole("button", { name: "Stop selected run" }),
  ).toHaveCount(0);
  await page
    .getByRole("combobox", { name: "Activity agent" })
    .selectOption(SCOUT);
  await expect(page.getByTestId("thread-agent-transcript")).toContainText(
    "Scout retained transcript",
  );
  await expect(
    page.getByRole("button", { name: "Stop selected run" }),
  ).toHaveCount(0);
  await chooseDevRun(page);
  await page
    .getByRole("button", { name: "Stop selected run" })
    .evaluate((button) => {
      (button as HTMLButtonElement).click();
      (button as HTMLButtonElement).click();
    });
  await expect(
    page.getByRole("status").filter({ hasText: "Stop signal accepted" }),
  ).toBeVisible();
  const state = await controlFixtureState(page);
  expect(state.controls).toHaveLength(1);
  expect(state.controls[0]).toMatchObject({
    agentPubkey: DEV,
    payload: {
      channelId: CHANNEL,
      conversationId: deriveAgentConversationId(CHANNEL, ROOT),
      turnId: `turn-${DEV}`,
    },
  });
  expect(state.controls[0].payload.requestId).toMatch(/^[0-9a-f-]{36}$/);
  expect(state.controls[0].expectedScope.scope.owner).toBeTruthy();
  await waitForAnimations(page);
  await page.getByRole("region", { name: "Thread activity" }).screenshot({
    path: "test-results/companyos-thread-workspace/mock-stop-accepted.png",
  });
  await expect(page.getByTestId("thread-agent-transcript")).toContainText(
    "Dev retained transcript",
  );
});

test("mock Activity exposes unconfirmed Stop and explicit reopen recovery", async ({
  page,
}) => {
  await page.clock.install();
  await page.setViewportSize({ width: 1800, height: 1000 });
  await setup(page);
  await installControlFixture(page);
  await injectWork(page, DEV, "Dev");
  await page.evaluate(() => {
    (
      window as typeof window & { __CREW354_CONTROLS__: { mode: string } }
    ).__CREW354_CONTROLS__.mode = "unknown";
  });
  await openActivity(page);
  await chooseDevRun(page);
  await page.getByRole("button", { name: "Stop selected run" }).click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Waiting for the selected run" }),
  ).toBeVisible();
  await page.clock.fastForward(10_001);
  await expect(
    page.getByRole("status").filter({ hasText: "Stop is unconfirmed" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Stop selected run" }),
  ).toBeDisabled();
  await waitForAnimations(page);
  await page.getByRole("region", { name: "Thread activity" }).screenshot({
    path: "test-results/companyos-thread-workspace/mock-stop-unconfirmed.png",
  });
  const first = (await controlFixtureState(page)).controls[0].payload.requestId;
  await page.getByRole("tab", { name: "Context", exact: true }).click();
  await page.getByRole("tab", { name: "Activity", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Stop selected run" }),
  ).toHaveCount(0);
  await page.evaluate(() => {
    (
      window as typeof window & { __CREW354_CONTROLS__: { mode: string } }
    ).__CREW354_CONTROLS__.mode = "accepted";
  });
  await chooseDevRun(page);
  await page.getByRole("button", { name: "Stop selected run" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Stop signal accepted" }),
  ).toBeVisible();
  const state = await controlFixtureState(page);
  expect(state.controls).toHaveLength(2);
  expect(state.controls[1].payload.requestId).not.toBe(first);
});

test("mock Need-you keeps a failed durable question retryable and publishes one answer", async ({
  page,
}) => {
  await setup(page);
  await installControlFixture(page);
  const requestId = "d".repeat(64);
  await page.waitForFunction(() =>
    window.__BUZZ_E2E_HAS_MOCK_LIVE_SUBSCRIPTION__?.({
      channelName: "general",
      kind: 46040,
    }),
  );
  await page.evaluate(
    ({ root, agent, channel, request }) => {
      (
        window as typeof window & {
          __CREW354_CONTROLS__: { answerFailure: boolean };
        }
      ).__CREW354_CONTROLS__.answerFailure = true;
      window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT__?.({
        channelName: "general",
        requestId: request,
        rootEventId: root,
        pubkey: agent,
        content: JSON.stringify({
          request_id: "crew354-question",
          session_id: `session-${agent}`,
          turn_id: `turn-${agent}`,
          channel_id: channel,
          engine: "claude",
          questions: [
            {
              id: "q",
              header: "Environment",
              question: "Where should this run?",
              options: [
                {
                  value: "staging",
                  label: "Staging",
                  description: "Isolated test environment",
                },
              ],
            },
          ],
        }),
      });
    },
    { root: ROOT, agent: DEV, channel: CHANNEL, request: requestId },
  );
  const card = page.getByTestId(`channel-user-input-card-${requestId}`);
  await expect(card).toBeVisible();
  await card.getByRole("radio", { name: "Staging" }).check();
  await card.getByTestId("channel-user-input-submit").click();
  await expect(card).toContainText("Mock answer publication failed");
  await waitForAnimations(page);
  await card.screenshot({
    path: "test-results/companyos-thread-workspace/mock-need-you-publication-error.png",
  });
  await expect(card.getByTestId("channel-user-input-submit")).toBeEnabled();
  await page.evaluate(() => {
    (
      window as typeof window & {
        __CREW354_CONTROLS__: { answerFailure: boolean };
      }
    ).__CREW354_CONTROLS__.answerFailure = false;
  });
  await card.getByTestId("channel-user-input-submit").evaluate((button) => {
    (button as HTMLButtonElement).click();
    (button as HTMLButtonElement).click();
  });
  await expect(card).toContainText("Sent, waiting for the agent.");
  expect((await controlFixtureState(page)).answers).toHaveLength(2); // one failed attempt, one successful publication
  await page.evaluate(
    ({ request, agent }) =>
      window.__BUZZ_E2E_EMIT_MOCK_USER_INPUT_RESOLVED__?.({
        channelName: "general",
        requestEventId: request,
        outcome: "answered",
        requestAgentPubkey: agent,
      }),
    { request: requestId, agent: DEV },
  );
  await expect(card).toContainText("Question answered");
  await expect(card.getByTestId("channel-user-input-submit")).toHaveCount(0);
});
