import assert from "node:assert/strict";
import { test } from "node:test";

import {
  collectConversationActivityHeadlines,
  getLiveAgentsForConversation,
  pickMostRecentlyActiveAgent,
  resetThreadAgentActivityHeadlineCaches,
  selectConversationActivityHeadline,
} from "./conversationActivityHeadline.ts";
import {
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "@/features/agents/activeAgentTurnsStore";
import { buildTranscript } from "@/features/agents/ui/agentSessionTranscript";

const CONV_A = "conversation-a";
const CONV_B = "conversation-b";
const AGENT_A =
  "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const AGENT_B =
  "bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222";
const NOW = Date.parse("2026-08-07T00:00:00.000Z");

const metadataSystemPrompt = {
  id: "meta:system",
  type: "metadata",
  renderClass: "raw-rail",
  title: "System prompt",
  sections: [{ title: "System", body: "Be helpful" }],
  timestamp: "2026-08-07T00:00:01.000Z",
  conversationId: CONV_A,
  turnId: "turn-a",
  acpSource: "session_prompt",
};

function makeTool(overrides = {}) {
  return {
    id: "tool:1",
    type: "tool",
    renderClass: "shell",
    title: "Shell",
    toolName: "dev__shell",
    buzzToolName: null,
    status: "executing",
    args: { command: "cargo test" },
    result: "",
    isError: false,
    timestamp: "2026-08-07T00:00:02.000Z",
    startedAt: "2026-08-07T00:00:02.000Z",
    completedAt: null,
    conversationId: CONV_A,
    turnId: "turn-a",
    descriptor: {
      renderClass: "shell",
      label: "Running",
      preview: "cargo test",
      source: "shell",
    },
    ...overrides,
  };
}

function makeMessage(overrides = {}) {
  return {
    id: "msg:1",
    type: "message",
    renderClass: "message",
    role: "assistant",
    title: "Assistant",
    text: "Checking the suite next.",
    timestamp: "2026-08-07T00:00:03.000Z",
    conversationId: CONV_A,
    turnId: "turn-a",
    ...overrides,
  };
}

test("collectConversationActivityHeadlines filters by conversationId (spine scan)", () => {
  resetThreadAgentActivityHeadlineCaches();
  const otherConversationTool = makeTool({
    id: "tool:other",
    conversationId: CONV_B,
    turnId: "turn-b",
    descriptor: {
      renderClass: "shell",
      label: "Running",
      preview: "npm test",
      source: "shell",
    },
  });
  const transcript = [
    metadataSystemPrompt,
    makeTool(),
    makeMessage(),
    otherConversationTool,
  ];

  const headlines = collectConversationActivityHeadlines(transcript, CONV_A);

  assert.ok(
    !headlines.includes("System prompt"),
    "metadata recedes when spine work exists",
  );
  assert.ok(
    headlines.some((h) => h.includes("cargo test")),
    "conversation-A tool headline present",
  );
  assert.ok(
    headlines.some((h) => h.includes("Checking the suite")),
    "assistant message headline present",
  );
  assert.ok(
    !headlines.some((h) => h.includes("npm test")),
    "conversation-B tool excluded",
  );
});

test("collectConversationActivityHeadlines falls back to turnIds when items lack conversationId", () => {
  resetThreadAgentActivityHeadlineCaches();
  const byTurn = makeTool({
    id: "tool:turn",
    conversationId: null,
    turnId: "turn-a",
  });
  const otherTurn = makeTool({
    id: "tool:other-turn",
    conversationId: null,
    turnId: "turn-z",
    descriptor: {
      renderClass: "shell",
      label: "Running",
      preview: "echo hi",
      source: "shell",
    },
  });

  const headlines = collectConversationActivityHeadlines(
    [byTurn, otherTurn],
    CONV_A,
    new Set(["turn-a"]),
  );

  assert.ok(headlines.some((h) => h.includes("cargo test")));
  assert.ok(!headlines.some((h) => h.includes("echo hi")));
});

test("collectConversationActivityHeadlines is reference-stable for the same transcript state", () => {
  resetThreadAgentActivityHeadlineCaches();
  const transcript = [makeTool(), makeMessage()];

  const first = collectConversationActivityHeadlines(transcript, CONV_A);
  const second = collectConversationActivityHeadlines(transcript, CONV_A);
  assert.equal(first, second, "same transcript reference → same result ref");

  // Content-identical rebuild still returns the prior reference.
  const rebuilt = [makeTool(), makeMessage()];
  const third = collectConversationActivityHeadlines(rebuilt, CONV_A);
  assert.equal(
    third,
    first,
    "identical headline content → same result reference",
  );
});

test("pickMostRecentlyActiveAgent picks newest lastActivityAt", () => {
  const agents = [
    {
      agentPubkey: AGENT_A,
      lastActivityAt: NOW - 10_000,
      turnIds: ["turn-a"],
    },
    {
      agentPubkey: AGENT_B,
      lastActivityAt: NOW - 1_000,
      turnIds: ["turn-b"],
    },
  ];
  const picked = pickMostRecentlyActiveAgent(agents);
  assert.ok(picked);
  assert.equal(picked.agentPubkey, AGENT_B);
});

test("live-agent selection sees in-place activity updates without a generation bump", async () => {
  resetActiveAgentTurnsStore();
  resetThreadAgentActivityHeadlineCaches();
  const base = {
    timestamp: "2026-08-07T00:00:00.000Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "channel-a",
    conversationId: CONV_A,
    sessionId: "session",
    payload: {},
  };

  // Start B first, then A, so A is initially the most recently active agent.
  syncAgentTurnsFromEvents(AGENT_B, [{ ...base, seq: 1, turnId: "turn-b" }]);
  await new Promise((resolve) => setTimeout(resolve, 2));
  syncAgentTurnsFromEvents(AGENT_A, [{ ...base, seq: 1, turnId: "turn-a" }]);
  assert.equal(
    pickMostRecentlyActiveAgent(getLiveAgentsForConversation(CONV_A))
      ?.agentPubkey,
    AGENT_A,
  );

  // recordActivity mutates B's turn in place and notifies, but intentionally
  // does not bump the active-turn generation.
  await new Promise((resolve) => setTimeout(resolve, 2));
  syncAgentTurnsFromEvents(AGENT_B, [
    {
      ...base,
      seq: 2,
      kind: "acp_read",
      turnId: "turn-b",
    },
  ]);
  const agents = getLiveAgentsForConversation(CONV_A);
  assert.equal(pickMostRecentlyActiveAgent(agents)?.agentPubkey, AGENT_B);
  resetActiveAgentTurnsStore();
});

test("selectConversationActivityHeadline prefixes short name for multi-agent threads", () => {
  resetThreadAgentActivityHeadlineCaches();
  const transcript = [makeTool()];
  const agents = [
    {
      agentPubkey: AGENT_A,
      lastActivityAt: NOW - 5_000,
      turnIds: ["turn-a"],
    },
    {
      agentPubkey: AGENT_B,
      lastActivityAt: NOW - 500,
      turnIds: ["turn-b"],
    },
  ];
  const profiles = {
    [AGENT_B]: {
      displayName: "Codex Prime",
      name: "codex",
      avatarUrl: null,
      nip05Handle: null,
      isAgent: true,
      ownerPubkey: null,
    },
  };

  const selection = selectConversationActivityHeadline(
    transcript,
    CONV_A,
    agents,
    profiles,
  );
  assert.ok(selection);
  assert.equal(selection.agentPubkey, AGENT_B);
  assert.equal(selection.prefixAgentName, true);
  assert.equal(selection.agentShortName, "Codex");
  // AGENT_B is most recent but transcript is AGENT_A's tools tagged CONV_A —
  // conversationId match still surfaces the headline, prefixed with B's name.
  assert.ok(selection.latest?.startsWith("Codex · "));
  assert.ok(selection.latest?.includes("cargo test"));
});

test("selectConversationActivityHeadline uses the selected agent's transcript", () => {
  resetThreadAgentActivityHeadlineCaches();
  const transcript = buildTranscript([
    {
      seq: 1,
      timestamp: "2026-08-07T00:00:02.000Z",
      kind: "acp_read",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: CONV_A,
      sessionId: "session-b",
      turnId: "turn-b",
      payload: {
        method: "session/update",
        params: {
          update: {
            sessionUpdate: "tool_call",
            toolCallId: "tool-b",
            status: "executing",
            title: "shell",
            kind: "shell",
            rawInput: { command: "npm test" },
          },
        },
      },
    },
  ]);
  const selection = selectConversationActivityHeadline(
    transcript,
    CONV_A,
    [
      {
        agentPubkey: AGENT_A,
        lastActivityAt: NOW - 10_000,
        turnIds: ["turn-a"],
      },
      {
        agentPubkey: AGENT_B,
        lastActivityAt: NOW,
        turnIds: ["turn-b"],
      },
    ],
    {
      [AGENT_B]: {
        displayName: "Codex Prime",
        name: "codex",
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: null,
      },
    },
  );

  assert.ok(selection);
  assert.equal(selection.agentPubkey, AGENT_B);
  assert.ok(selection.latest?.startsWith("Codex · "));
  assert.ok(selection.latest?.includes("npm test"));
  assert.ok(!selection.latest?.includes("cargo test"));
});

test("selectConversationActivityHeadline omits name prefix for a single agent", () => {
  resetThreadAgentActivityHeadlineCaches();
  const selection = selectConversationActivityHeadline(
    [makeTool()],
    CONV_A,
    [
      {
        agentPubkey: AGENT_A,
        lastActivityAt: NOW,
        turnIds: ["turn-a"],
      },
    ],
    {
      [AGENT_A]: {
        displayName: "Claude Opus",
        name: "claude",
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: null,
      },
    },
  );
  assert.ok(selection);
  assert.equal(selection.prefixAgentName, false);
  assert.ok(!selection.latest?.startsWith("Claude · "));
  assert.ok(selection.latest?.includes("cargo test"));
});

test("selectConversationActivityHeadline is reference-stable across identical inputs", () => {
  resetThreadAgentActivityHeadlineCaches();
  const transcript = [makeTool(), makeMessage()];
  const agents = [
    {
      agentPubkey: AGENT_A,
      lastActivityAt: NOW,
      turnIds: ["turn-a"],
    },
  ];
  const first = selectConversationActivityHeadline(
    transcript,
    CONV_A,
    agents,
    undefined,
  );
  const second = selectConversationActivityHeadline(
    transcript,
    CONV_A,
    agents,
    undefined,
  );
  assert.equal(first, second);
  assert.equal(first?.headlines, second?.headlines);
});
