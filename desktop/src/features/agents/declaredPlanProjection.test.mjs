import assert from "node:assert/strict";
import test from "node:test";

import {
  collectParticipatingAgentPubkeys,
  latestSessionIdFromEvents,
  projectAgentDeclaredPlan,
  projectDeclaredPlansForThread,
} from "./declaredPlanProjection.ts";

const DEV = "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const SCOUT =
  "bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222";
const CLAUDE =
  "cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333";
const CONV = "11111111-2222-3333-4444-555555555555";
const OTHER = "99999999-8888-7777-6666-555555555555";

function planEvent(agentIndex, seq, entries, overrides = {}) {
  return {
    seq,
    timestamp: `2026-08-13T10:00:0${seq}.000Z`,
    kind: "acp_read",
    agentIndex,
    channelId: "channel-a",
    conversationId: CONV,
    sessionId: "sess-live",
    turnId: "turn-1",
    payload: {
      method: "session/update",
      params: {
        sessionId: "sess-live",
        update: { sessionUpdate: "plan", entries },
      },
    },
    ...overrides,
  };
}

function todoEvent(agentIndex, seq, todos, overrides = {}) {
  return {
    seq,
    timestamp: `2026-08-13T10:00:0${seq}.000Z`,
    kind: "acp_read",
    agentIndex,
    channelId: "channel-a",
    conversationId: CONV,
    sessionId: "sess-live",
    turnId: "turn-1",
    payload: {
      method: "session/update",
      params: {
        sessionId: "sess-live",
        update: {
          sessionUpdate: "tool_call",
          title: "todo",
          toolName: "todo",
          rawInput: { todos },
        },
      },
    },
    ...overrides,
  };
}

test("two same-ACP agents keep independent rail cards", () => {
  const plans = projectDeclaredPlansForThread(CONV, [
    {
      agentPubkey: DEV,
      agentName: "Hermes Dev",
      liveness: "working",
      events: [
        planEvent(0, 1, [
          { content: "Fetch tags", status: "completed" },
          { content: "Compare ACP lifecycle", status: "in_progress" },
        ]),
      ],
    },
    {
      agentPubkey: SCOUT,
      agentName: "Hermes Scout",
      liveness: "sleeping",
      events: [
        planEvent(1, 1, [
          { content: "Inventory existing todo seams", status: "completed" },
        ]),
      ],
    },
  ]);

  assert.equal(plans.length, 2);
  assert.deepEqual(
    plans[0].entries.map((entry) => entry.content),
    ["Fetch tags", "Compare ACP lifecycle"],
  );
  const scoutUpdate = projectAgentDeclaredPlan(CONV, {
    agentPubkey: SCOUT,
    agentName: "Hermes Scout",
    liveness: "sleeping",
    events: [
      planEvent(1, 1, [
        { content: "Inventory existing todo seams", status: "completed" },
      ]),
      planEvent(1, 2, [
        { content: "Inventory existing todo seams", status: "completed" },
        { content: "Confirm Claude/Codex payloads", status: "pending" },
      ]),
    ],
  });
  assert.equal(scoutUpdate.entries.length, 2);
  assert.deepEqual(
    plans[0].entries.map((entry) => entry.content),
    ["Fetch tags", "Compare ACP lifecycle"],
  );
  assert.equal(plans[0].agentName, "Hermes Dev");
});

test("a later plan snapshot replaces that agent wholesale including removals", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: DEV,
    agentName: "Hermes Dev",
    liveness: "working",
    events: [
      planEvent(0, 1, [
        { content: "Keep", status: "pending" },
        { content: "Drop me", status: "pending" },
      ]),
      planEvent(0, 2, [{ content: "Keep", status: "completed" }]),
    ],
  });
  assert.deepEqual(plan.entries, [{ content: "Keep", status: "completed" }]);
});

test("empty entries clears the card instead of keeping stale rows", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: DEV,
    agentName: "Hermes Dev",
    liveness: "working",
    events: [
      planEvent(0, 1, [{ content: "Stale", status: "pending" }]),
      planEvent(0, 2, []),
    ],
  });
  assert.equal(plan.unknown, true);
  assert.deepEqual(plan.entries, []);
});

test("no plan and no structured todo is muted unknown, never a guessed checklist", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: CLAUDE,
    agentName: "Claude",
    liveness: "disconnected",
    events: [
      {
        seq: 1,
        timestamp: "2026-08-13T10:00:00.000Z",
        kind: "acp_read",
        agentIndex: 2,
        channelId: "channel-a",
        conversationId: CONV,
        sessionId: "sess-live",
        turnId: "turn-1",
        payload: {
          method: "session/update",
          params: {
            update: {
              sessionUpdate: "agent_message_chunk",
              content: {
                type: "text",
                text: "- [ ] Guessed from prose\n- [ ] Another fake task",
              },
            },
          },
        },
      },
    ],
  });
  assert.equal(plan.unknown, true);
  assert.deepEqual(plan.entries, []);
});

test("identical text from two agents remains two rows under two headings", () => {
  const plans = projectDeclaredPlansForThread(CONV, [
    {
      agentPubkey: DEV,
      agentName: "Hermes Dev",
      liveness: "working",
      events: [
        planEvent(0, 1, [
          { content: "Write the sync issue", status: "pending" },
        ]),
      ],
    },
    {
      agentPubkey: SCOUT,
      agentName: "Hermes Scout",
      liveness: "idle",
      events: [
        planEvent(1, 1, [
          { content: "Write the sync issue", status: "pending" },
        ]),
      ],
    },
  ]);
  assert.equal(plans[0].entries[0].content, plans[1].entries[0].content);
  assert.notEqual(plans[0].agentPubkey, plans[1].agentPubkey);
  assert.equal(plans[0].agentName, "Hermes Dev");
  assert.equal(plans[1].agentName, "Hermes Scout");
});

test("after idle spin-down the last declared snapshot stays and is labeled sleeping", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: SCOUT,
    agentName: "Hermes Scout",
    liveness: "sleeping",
    liveSessionId: "sess-live",
    events: [
      planEvent(1, 1, [
        { content: "Inventory existing todo seams", status: "completed" },
        { content: "Confirm Claude/Codex payloads", status: "pending" },
      ]),
    ],
  });
  assert.equal(plan.unknown, false);
  assert.equal(plan.liveness, "sleeping");
  assert.equal(plan.entries.length, 2);
  assert.equal(plan.source, "acp-plan");
});

test("stale-lineage / failed session load drops the dead session snapshot", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: DEV,
    agentName: "Hermes Dev",
    liveness: "idle",
    liveSessionId: "sess-rebuilt",
    retiredSessionIds: new Set(["sess-dead"]),
    events: [
      planEvent(
        0,
        1,
        [{ content: "Dead session work", status: "in_progress" }],
        {
          sessionId: "sess-dead",
        },
      ),
      {
        seq: 2,
        timestamp: "2026-08-13T10:00:02.000Z",
        kind: "turn_started",
        agentIndex: 0,
        channelId: "channel-a",
        conversationId: CONV,
        sessionId: "sess-rebuilt",
        turnId: "turn-2",
        payload: { source: "channel" },
      },
    ],
  });
  assert.equal(plan.unknown, true);
  assert.deepEqual(plan.entries, []);
});

test("todo-tool fallback is same-agent only and does not merge across agents", () => {
  const plans = projectDeclaredPlansForThread(CONV, [
    {
      agentPubkey: DEV,
      agentName: "Hermes Dev",
      liveness: "working",
      events: [todoEvent(0, 1, [{ text: "Shared title", done: false }])],
    },
    {
      agentPubkey: SCOUT,
      agentName: "Hermes Scout",
      liveness: "idle",
      events: [todoEvent(1, 1, [{ text: "Shared title", done: true }])],
    },
  ]);
  assert.equal(plans[0].source, "todo-tool");
  assert.equal(plans[0].entries[0].status, "pending");
  assert.equal(plans[1].entries[0].status, "completed");
});

test("events for another conversation never leak into this thread", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: DEV,
    agentName: "Hermes Dev",
    liveness: "working",
    events: [
      planEvent(0, 1, [{ content: "Other thread", status: "pending" }], {
        conversationId: OTHER,
      }),
    ],
  });
  assert.equal(plan.unknown, true);
});

test("latestSessionIdFromEvents uses the live window, not an older archived session", () => {
  assert.equal(
    latestSessionIdFromEvents(
      [
        planEvent(0, 1, [{ content: "Dead", status: "pending" }], {
          sessionId: "sess-dead",
        }),
        {
          seq: 2,
          timestamp: "2026-08-13T10:00:02.000Z",
          kind: "turn_started",
          agentIndex: 0,
          channelId: "channel-a",
          conversationId: CONV,
          sessionId: "sess-rebuilt",
          turnId: "turn-2",
          payload: { source: "channel" },
        },
      ],
      CONV,
    ),
    "sess-rebuilt",
  );
});

test("collectParticipatingAgentPubkeys lists mentioned known agents without inventing others", () => {
  const pubkeys = collectParticipatingAgentPubkeys({
    knownAgentPubkeys: new Set([DEV, SCOUT]),
    messages: [
      {
        id: "a".repeat(64),
        createdAt: 1,
        pubkey: "ffff".repeat(16),
        author: "Founder",
        body: "please look",
        tags: [
          ["p", DEV],
          ["p", SCOUT],
        ],
      },
    ],
  });
  assert.deepEqual(pubkeys, [DEV, SCOUT]);
});
