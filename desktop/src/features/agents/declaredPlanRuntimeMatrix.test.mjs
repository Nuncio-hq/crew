import assert from "node:assert/strict";
import test from "node:test";

import { projectAgentDeclaredPlan } from "./declaredPlanProjection.ts";
import { snapshotFromObserverEvent } from "./declaredPlanSnapshot.ts";

const AGENT =
  "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const CONV = "11111111-2222-3333-4444-555555555555";

function acpReadEvent(seq, update, overrides = {}) {
  return {
    seq,
    timestamp: `2026-08-13T10:00:${String(seq).padStart(2, "0")}.000Z`,
    kind: "acp_read",
    agentIndex: 0,
    channelId: "channel-a",
    conversationId: CONV,
    sessionId: "sess-live",
    turnId: "turn-1",
    payload: {
      method: "session/update",
      params: { sessionId: "sess-live", update },
    },
    ...overrides,
  };
}

test("Hermes/Codex native sessionUpdate:plan replaces wholesale", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: AGENT,
    liveness: "working",
    liveSessionId: "sess-live",
    events: [
      acpReadEvent(1, {
        sessionUpdate: "plan",
        entries: [
          { content: "Fetch tags", status: "completed" },
          { content: "Compare lifecycle", status: "pending" },
        ],
      }),
      acpReadEvent(2, {
        sessionUpdate: "plan",
        entries: [{ content: "Compare lifecycle", status: "completed" }],
      }),
    ],
  });
  assert.equal(plan.source, "acp-plan");
  assert.deepEqual(plan.entries, [
    { content: "Compare lifecycle", status: "completed" },
  ]);
});

test("Claude TodoWrite tool_call maps content+status rows", () => {
  const snapshot = snapshotFromObserverEvent(
    acpReadEvent(3, {
      sessionUpdate: "tool_call",
      title: "TodoWrite",
      rawInput: {
        todos: [
          { content: "Draft the note", status: "in_progress" },
          { content: "Ship it", status: "pending" },
        ],
      },
    }),
  );
  assert.equal(snapshot?.source, "todo-tool");
  assert.deepEqual(snapshot?.entries, [
    { content: "Draft the note", status: "in_progress" },
    { content: "Ship it", status: "pending" },
  ]);
});

test("Buzz Agent buzz-dev-mcp todo tool uses text+done fallback", () => {
  const snapshot = snapshotFromObserverEvent(
    acpReadEvent(4, {
      sessionUpdate: "tool_call",
      title: "todo",
      toolName: "todo",
      rawInput: {
        todos: [
          { text: "Inventory seams", done: true },
          { text: "Confirm payloads", done: false },
        ],
      },
    }),
  );
  assert.equal(snapshot?.source, "todo-tool");
  assert.deepEqual(snapshot?.entries, [
    { content: "Inventory seams", status: "completed" },
    { content: "Confirm payloads", status: "pending" },
  ]);
});

test("tool_call_update without todos is not a declared-plan snapshot", () => {
  assert.equal(
    snapshotFromObserverEvent(
      acpReadEvent(5, {
        sessionUpdate: "tool_call_update",
        toolCallId: "call-1",
        status: "completed",
        title: "todo",
      }),
    ),
    null,
  );
});

test("status-only tool_call_update does not clobber an earlier todo snapshot", () => {
  const plan = projectAgentDeclaredPlan(CONV, {
    agentPubkey: AGENT,
    liveness: "working",
    liveSessionId: "sess-live",
    events: [
      acpReadEvent(1, {
        sessionUpdate: "tool_call",
        title: "todo",
        toolName: "todo",
        rawInput: {
          todos: [{ text: "Keep me", done: false }],
        },
      }),
      acpReadEvent(6, {
        sessionUpdate: "tool_call_update",
        toolCallId: "call-1",
        status: "completed",
        title: "todo",
        timestamp: "2026-08-13T10:00:06.000Z",
      }),
    ],
  });
  assert.equal(plan.source, "todo-tool");
  assert.deepEqual(plan.entries, [{ content: "Keep me", status: "pending" }]);
});

test("Grok plan.md search_replace is not a declared plan", () => {
  assert.equal(
    snapshotFromObserverEvent(
      acpReadEvent(7, {
        sessionUpdate: "tool_call",
        title: "search_replace",
        rawInput: {
          file_path: "/sessions/plan.md",
          new_string: "# Plan\n- [ ] Step one\n",
        },
      }),
    ),
    null,
  );
});

test("CreatePlan tool_call without structured todos is not a declared plan", () => {
  assert.equal(
    snapshotFromObserverEvent(
      acpReadEvent(8, {
        sessionUpdate: "tool_call",
        title: "CreatePlan",
        kind: "other",
      }),
    ),
    null,
  );
});

test("parseAcpPlanSnapshot rejects markdown-only plan updates", () => {
  assert.equal(
    snapshotFromObserverEvent(
      acpReadEvent(9, {
        sessionUpdate: "plan",
        content: { type: "text", text: "- [ ] Guessed from prose" },
      }),
    ),
    null,
  );
});
