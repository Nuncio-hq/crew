import { expect, test } from "@playwright/test";
import { schnorr } from "@noble/curves/secp256k1.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex, hexToBytes } from "@noble/hashes/utils.js";
import { finalizeEvent } from "nostr-tools/pure";

import { installBridge, TEST_IDENTITIES } from "../helpers/bridge";
import { assertRelaySeeded } from "../helpers/seed";

const RELAY_HTTP_URL =
  process.env.BUZZ_E2E_RELAY_URL ?? "http://localhost:3000";
/** Relay-seeded #general channel id (uuid5 of buzz.channel.general). */
const GENERAL_CHANNEL_ID = "9f28288a-d724-587a-9709-92dc7f967110";

type RelayEvent = {
  id: string;
  kind: number;
  pubkey: string;
  content: string;
  tags: string[][];
  created_at: number;
};

async function publishEvent(
  identity: { privateKey: string; pubkey: string },
  template: {
    kind: number;
    content: string;
    tags: string[][];
    created_at?: number;
  },
  extraHeaders: Record<string, string> = {},
): Promise<RelayEvent> {
  const event = finalizeEvent(
    {
      kind: template.kind,
      content: template.content,
      tags: template.tags,
      created_at: template.created_at ?? Math.floor(Date.now() / 1000),
    },
    hexToBytes(identity.privateKey),
  );
  const url = `${RELAY_HTTP_URL}/events`;
  const body = JSON.stringify(event);
  // NIP-OA ownership backfill requires a verified authentication timestamp.
  const auth = finalizeEvent(
    {
      kind: 27235,
      created_at: Math.floor(Date.now() / 1000),
      content: "",
      tags: [
        ["u", url],
        ["method", "POST"],
        ["payload", bytesToHex(sha256(new TextEncoder().encode(body)))],
      ],
    },
    hexToBytes(identity.privateKey),
  );
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Nostr ${Buffer.from(JSON.stringify(auth)).toString("base64")}`,
      ...extraHeaders,
    },
    body,
  });
  if (!response.ok) {
    throw new Error(
      `POST /events failed (${response.status}): ${await response.text()}`,
    );
  }
  return event as RelayEvent;
}

async function queryRelay(
  filters: Array<Record<string, unknown>>,
  asPubkey = TEST_IDENTITIES.tyler.pubkey,
): Promise<RelayEvent[]> {
  const response = await fetch(`${RELAY_HTTP_URL}/query`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Pubkey": asPubkey,
    },
    body: JSON.stringify(filters),
  });
  if (!response.ok) {
    throw new Error(
      `POST /query failed (${response.status}): ${await response.text()}`,
    );
  }
  return (await response.json()) as RelayEvent[];
}

/** NIP-OA auth tag JSON: tyler owns `agentPubkey`. */
function computeOwnerAuthTagJson(agentPubkey: string): string {
  const conditions = "";
  const preimage = new TextEncoder().encode(
    `nostr:agent-auth:${agentPubkey.toLowerCase()}:${conditions}`,
  );
  const message = sha256(preimage);
  const sig = bytesToHex(
    schnorr.sign(message, hexToBytes(TEST_IDENTITIES.tyler.privateKey)),
  );
  return JSON.stringify([
    "auth",
    TEST_IDENTITIES.tyler.pubkey,
    conditions,
    sig,
  ]);
}

async function invokeBridgeCommand(
  page: import("@playwright/test").Page,
  command: string,
  payload?: Record<string, unknown>,
): Promise<unknown> {
  await page.waitForFunction(
    () => {
      const w = window as Window & {
        __BUZZ_E2E_INVOKE_MOCK_COMMAND__?: unknown;
        __TAURI_INTERNALS__?: { invoke?: unknown };
      };
      return (
        typeof w.__BUZZ_E2E_INVOKE_MOCK_COMMAND__ === "function" ||
        typeof w.__TAURI_INTERNALS__?.invoke === "function"
      );
    },
    null,
    { timeout: 10_000 },
  );
  return page.evaluate(
    async ({ command: cmd, payload: pl }) => {
      const w = window as Window & {
        __BUZZ_E2E_INVOKE_MOCK_COMMAND__?: (
          command: string,
          payload?: Record<string, unknown>,
        ) => Promise<unknown>;
        __TAURI_INTERNALS__?: {
          invoke?: (
            command: string,
            payload?: Record<string, unknown>,
          ) => Promise<unknown>;
        };
      };
      const invoke =
        w.__BUZZ_E2E_INVOKE_MOCK_COMMAND__ ?? w.__TAURI_INTERNALS__?.invoke;
      if (!invoke) throw new Error("Mock invoke bridge is unavailable.");
      return invoke(cmd, pl);
    },
    { command, payload },
  );
}

test.beforeAll(async () => {
  await assertRelaySeeded();
});

test("archive_identity and unarchive_identity publish real kind 9035/9036", async ({
  page,
}) => {
  await installBridge(page, {
    mode: "relay",
    user: "tyler",
    relayHttpUrl: RELAY_HTTP_URL,
    relayWsUrl: RELAY_HTTP_URL.replace(/^http/, "ws"),
  });
  await page.goto("/");
  await expect(page.getByTestId("app-sidebar")).toBeVisible({
    timeout: 15_000,
  });

  const target = TEST_IDENTITIES.tyler.pubkey;
  const reason = `bridge-relay-self-${Date.now()}`;

  const archiveResult = (await invokeBridgeCommand(page, "archive_identity", {
    req: {
      targetPubkey: target,
      content: "self archive via bridge",
      reason,
    },
  })) as { event_id?: string; accepted?: boolean };

  expect(archiveResult?.accepted ?? true).toBeTruthy();

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [9035],
              authors: [target],
              "#p": [target],
              limit: 20,
            },
          ])
        ).find(
          (event) =>
            event.kind === 9035 &&
            event.pubkey === target &&
            event.tags.some((tag) => tag[0] === "-") &&
            event.tags.some(
              (tag) =>
                tag[0] === "p" &&
                tag[1]?.toLowerCase() === target.toLowerCase(),
            ) &&
            event.tags.some((tag) => tag[0] === "reason" && tag[1] === reason),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();

  const unarchiveResult = (await invokeBridgeCommand(
    page,
    "unarchive_identity",
    {
      req: {
        targetPubkey: target,
        content: "self unarchive via bridge",
        reason: `un-${reason}`,
      },
    },
  )) as { event_id?: string; accepted?: boolean };
  expect(unarchiveResult?.accepted ?? true).toBeTruthy();

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [9036],
              authors: [target],
              "#p": [target],
              limit: 20,
            },
          ])
        ).find(
          (event) =>
            event.kind === 9036 &&
            event.pubkey === target &&
            event.tags.some((tag) => tag[0] === "-") &&
            event.tags.some(
              (tag) =>
                tag[0] === "p" &&
                tag[1]?.toLowerCase() === target.toLowerCase(),
            ),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();
});

test("update_persona_and_publish posts a real kind 30175 catalog head", async ({
  page,
}) => {
  const personaId = `bridge-relay-persona-${Date.now().toString(36)}`;
  const displayName = `Relay Persona ${Date.now()}`;

  await installBridge(page, {
    mode: "relay",
    user: "tyler",
    relayHttpUrl: RELAY_HTTP_URL,
    relayWsUrl: RELAY_HTTP_URL.replace(/^http/, "ws"),
    mock: {
      personas: [
        {
          id: personaId,
          displayName: "Before publish",
          systemPrompt: "You are a test persona.",
          shared: true,
          isActive: true,
        },
      ],
    },
  });
  await page.goto("/");
  await expect(page.getByTestId("app-sidebar")).toBeVisible({
    timeout: 15_000,
  });

  const result = (await invokeBridgeCommand(
    page,
    "update_persona_and_publish",
    {
      input: {
        id: personaId,
        displayName,
        systemPrompt: "You are a relay-published test persona.",
      },
    },
  )) as {
    publicationStatus?: string;
    persona?: { display_name?: string; id?: string };
  };

  expect(result.publicationStatus).toBe("published");
  expect(result.persona?.display_name).toBe(displayName);

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [30175],
              authors: [TEST_IDENTITIES.tyler.pubkey],
              "#d": [personaId],
              limit: 5,
            },
          ])
        ).find((event) => {
          if (event.kind !== 30175) return false;
          if (event.pubkey !== TEST_IDENTITIES.tyler.pubkey) return false;
          if (
            !event.tags.some((tag) => tag[0] === "d" && tag[1] === personaId)
          ) {
            return false;
          }
          try {
            const body = JSON.parse(event.content) as {
              display_name?: string;
            };
            return body.display_name === displayName;
          } catch {
            return false;
          }
        }),
      { timeout: 15_000 },
    )
    .toBeTruthy();
});

test("send_managed_agent_channel_message publishes a real kind-9 as the agent", async ({
  page,
}) => {
  const content = `managed-agent relay message ${Date.now()}`;
  const marker = `bridge-managed-${Date.now()}`;

  await installBridge(page, {
    mode: "relay",
    user: "tyler",
    relayHttpUrl: RELAY_HTTP_URL,
    relayWsUrl: RELAY_HTTP_URL.replace(/^http/, "ws"),
    mock: {
      managedAgents: [
        {
          pubkey: TEST_IDENTITIES.alice.pubkey,
          name: "Alice Agent",
          privateKeyHex: TEST_IDENTITIES.alice.privateKey,
          status: "running",
          channelIds: [GENERAL_CHANNEL_ID],
        },
      ],
    },
  });
  await page.goto("/");
  await expect(page.getByTestId("app-sidebar")).toBeVisible({
    timeout: 15_000,
  });

  const result = (await invokeBridgeCommand(
    page,
    "send_managed_agent_channel_message",
    {
      agentPubkey: TEST_IDENTITIES.alice.pubkey,
      channelId: GENERAL_CHANNEL_ID,
      content,
      marker,
    },
  )) as { event_id?: string };

  expect(result.event_id).toMatch(/^[0-9a-f]{64}$/);

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [9],
              authors: [TEST_IDENTITIES.alice.pubkey],
              "#h": [GENERAL_CHANNEL_ID],
              limit: 20,
            },
          ])
        ).find(
          (event) =>
            event.id === result.event_id &&
            event.kind === 9 &&
            event.pubkey === TEST_IDENTITIES.alice.pubkey &&
            event.content === content &&
            event.tags.some(
              (tag) => tag[0] === "h" && tag[1] === GENERAL_CHANNEL_ID,
            ) &&
            event.tags.some((tag) => tag[0] === "client" && tag[1] === marker),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();
});

test("send_channel_user_input_answer publishes a real kind 46041", async ({
  page,
}) => {
  // Seed path mirrors production: materialize alice as tyler's agent via NIP-OA,
  // publish a parent that @-targets alice, then a 46040 request alice→tyler.
  const authTagJson = computeOwnerAuthTagJson(TEST_IDENTITIES.alice.pubkey);

  // Materialize agent_owner via x-auth-tag on any accepted alice-authored event.
  const authTag = JSON.parse(authTagJson) as [string, string, string, string];
  await publishEvent(
    TEST_IDENTITIES.alice,
    {
      kind: 0,
      content: JSON.stringify({
        name: "Alice Agent",
        about: "bridge-relay user-input agent",
      }),
      tags: [authTag],
    },
    { "x-auth-tag": authTagJson },
  );

  const parent = await publishEvent(TEST_IDENTITIES.tyler, {
    kind: 9,
    content: `trigger for user-input ${Date.now()}`,
    tags: [
      ["h", GENERAL_CHANNEL_ID],
      ["p", TEST_IDENTITIES.alice.pubkey],
    ],
  });

  const requestContent = JSON.stringify({
    request_id: `req-${Date.now()}`,
    session_id: "session-bridge-relay",
    turn_id: "turn-1",
    channel_id: GENERAL_CHANNEL_ID,
    tool_call_id: null,
    engine: "codex",
    message: "Pick one",
    questions: [
      {
        id: "q0",
        header: "Choice",
        question: "Which path?",
        options: [
          {
            value: "a",
            label: "A",
            description: "Option A",
          },
        ],
      },
    ],
  });

  const request = await publishEvent(
    TEST_IDENTITIES.alice,
    {
      kind: 46040,
      content: requestContent,
      tags: [
        ["h", GENERAL_CHANNEL_ID],
        ["p", TEST_IDENTITIES.tyler.pubkey],
        ["e", parent.id, "", "reply"],
      ],
    },
    { "x-auth-tag": authTagJson },
  );

  await installBridge(page, {
    mode: "relay",
    user: "tyler",
    relayHttpUrl: RELAY_HTTP_URL,
    relayWsUrl: RELAY_HTTP_URL.replace(/^http/, "ws"),
  });
  await page.goto("/");
  await expect(page.getByTestId("app-sidebar")).toBeVisible({
    timeout: 15_000,
  });

  const answers = { q0: "a" };
  const result = (await invokeBridgeCommand(
    page,
    "send_channel_user_input_answer",
    {
      channelId: GENERAL_CHANNEL_ID,
      requestEventId: request.id,
      answers,
    },
  )) as { event_id?: string; accepted?: boolean };

  expect(result.accepted).toBe(true);
  expect(result.event_id).toMatch(/^[0-9a-f]{64}$/);

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [46041],
              authors: [TEST_IDENTITIES.tyler.pubkey],
              "#e": [request.id],
              limit: 10,
            },
          ])
        ).find(
          (event) =>
            event.id === result.event_id &&
            event.kind === 46041 &&
            event.pubkey === TEST_IDENTITIES.tyler.pubkey &&
            event.content === JSON.stringify(answers) &&
            event.tags.some(
              (tag) => tag[0] === "h" && tag[1] === GENERAL_CHANNEL_ID,
            ) &&
            event.tags.some((tag) => tag[0] === "e" && tag[1] === request.id) &&
            event.tags.some(
              (tag) =>
                tag[0] === "p" &&
                tag[1]?.toLowerCase() ===
                  TEST_IDENTITIES.alice.pubkey.toLowerCase(),
            ),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();
});

test("workflow create/update/trigger/delete publish real 30620 / 46020 / 5", async ({
  page,
}) => {
  // Mirrors desktop/src-tauri/src/commands/workflows.rs + events.rs:
  // create/update → kind 30620 (d+h, YAML content); trigger → 46020 (d);
  // delete → kind 5 with a=30620:owner:id. Issue title mentioned 30625;
  // Rust has no such kind — only 30620.
  await installBridge(page, {
    mode: "relay",
    user: "tyler",
    relayHttpUrl: RELAY_HTTP_URL,
    relayWsUrl: RELAY_HTTP_URL.replace(/^http/, "ws"),
  });
  await page.goto("/");
  await expect(page.getByTestId("app-sidebar")).toBeVisible({
    timeout: 15_000,
  });

  const marker = `bridge-wf-${Date.now()}`;
  // TriggerDef accepts message_posted|reaction_added|diff_posted|schedule|webhook
  // (not "manual"). Use webhook + a valid delay step so relay YAML validation
  // accepts the definition (issue #177).
  const createYaml = [
    `name: ${marker}`,
    "trigger:",
    "  on: webhook",
    "steps:",
    "  - id: step_1",
    "    action: delay",
    "    duration: 1s",
  ].join("\n");

  const created = (await invokeBridgeCommand(page, "create_workflow", {
    channelId: GENERAL_CHANNEL_ID,
    yamlDefinition: createYaml,
  })) as {
    id?: string;
    revision?: string;
    created_at: number;
    name?: string;
    channel_id?: string;
    owner_pubkey?: string;
  };

  expect(created.id).toMatch(
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
  );
  expect(created.name).toBe(marker);
  expect(created.channel_id).toBe(GENERAL_CHANNEL_ID);
  expect(created.owner_pubkey?.toLowerCase()).toBe(
    TEST_IDENTITIES.tyler.pubkey.toLowerCase(),
  );

  const workflowId = created.id as string;

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [30620],
              authors: [TEST_IDENTITIES.tyler.pubkey],
              "#d": [workflowId],
              "#h": [GENERAL_CHANNEL_ID],
              limit: 5,
            },
          ])
        ).find(
          (event) =>
            event.kind === 30620 &&
            event.pubkey === TEST_IDENTITIES.tyler.pubkey &&
            event.content === createYaml &&
            event.tags.some((tag) => tag[0] === "d" && tag[1] === workflowId) &&
            event.tags.some(
              (tag) => tag[0] === "h" && tag[1] === GENERAL_CHANNEL_ID,
            ),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();

  const updatedName = `${marker}-updated`;
  const updateYaml = [
    `name: ${updatedName}`,
    "trigger:",
    "  on: webhook",
    "steps:",
    "  - id: step_1",
    "    action: delay",
    "    duration: 1s",
  ].join("\n");

  // Addressable events use second-resolution NIP-33 ordering. Exercise an
  // ordered update instead of randomly winning a same-second event-id tie.
  await expect
    .poll(() => Math.floor(Date.now() / 1000))
    .toBeGreaterThan(created.created_at);
  const updated = (await invokeBridgeCommand(page, "update_workflow", {
    workflowId,
    expectedRevision: created.revision,
    yamlDefinition: updateYaml,
  })) as { id?: string; name?: string };

  expect(updated.id).toBe(workflowId);
  expect(updated.name).toBe(updatedName);

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [30620],
              authors: [TEST_IDENTITIES.tyler.pubkey],
              "#d": [workflowId],
              limit: 5,
            },
          ])
        ).find(
          (event) =>
            event.kind === 30620 &&
            event.content === updateYaml &&
            event.tags.some(
              (tag) => tag[0] === "h" && tag[1] === GENERAL_CHANNEL_ID,
            ),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();

  const triggerResult = (await invokeBridgeCommand(page, "trigger_workflow", {
    workflowId,
  })) as { event_id?: string };

  expect(triggerResult.event_id).toMatch(/^[0-9a-f]{64}$/);

  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [46020],
              authors: [TEST_IDENTITIES.tyler.pubkey],
              "#d": [workflowId],
              limit: 10,
            },
          ])
        ).find(
          (event) =>
            event.id === triggerResult.event_id &&
            event.kind === 46020 &&
            event.content === "" &&
            event.tags.some((tag) => tag[0] === "d" && tag[1] === workflowId),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();

  await invokeBridgeCommand(page, "delete_workflow", { workflowId });

  const deleteCoord = `30620:${TEST_IDENTITIES.tyler.pubkey}:${workflowId}`;
  await expect
    .poll(
      async () =>
        (
          await queryRelay([
            {
              kinds: [5],
              authors: [TEST_IDENTITIES.tyler.pubkey],
              "#a": [deleteCoord],
              limit: 10,
            },
          ])
        ).find(
          (event) =>
            event.kind === 5 &&
            event.content === "" &&
            event.tags.some((tag) => tag[0] === "a" && tag[1] === deleteCoord),
        ),
      { timeout: 15_000 },
    )
    .toBeTruthy();
});
