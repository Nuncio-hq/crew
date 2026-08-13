import assert from "node:assert/strict";
import test from "node:test";

import {
  parseAcpPlanSnapshot,
  parseTodoToolSnapshot,
  snapshotFromObserverEvent,
} from "./declaredPlanSnapshot.ts";

const META = {
  updatedAt: "2026-08-13T10:00:00.000Z",
  sessionId: "sess-1",
  seq: 4,
};

test("parseAcpPlanSnapshot keeps pending, in_progress, and completed", () => {
  const snapshot = parseAcpPlanSnapshot(
    {
      sessionUpdate: "plan",
      entries: [
        { content: "Fetch tags", status: "completed" },
        { content: "Compare ACP lifecycle", status: "in_progress" },
        { content: "Write sync issue", status: "pending" },
      ],
    },
    META,
  );
  assert.deepEqual(snapshot?.entries, [
    { content: "Fetch tags", status: "completed" },
    { content: "Compare ACP lifecycle", status: "in_progress" },
    { content: "Write sync issue", status: "pending" },
  ]);
  assert.equal(snapshot?.source, "acp-plan");
});

test("parseAcpPlanSnapshot treats missing entries as not a snapshot", () => {
  assert.equal(
    parseAcpPlanSnapshot(
      {
        sessionUpdate: "plan",
        content: { type: "text", text: "- [ ] Guessed markdown" },
      },
      META,
    ),
    null,
  );
});

test("parseAcpPlanSnapshot keeps cancelled completed text", () => {
  const snapshot = parseAcpPlanSnapshot(
    {
      entries: [{ content: "[cancelled] Old step", status: "completed" }],
    },
    META,
  );
  assert.equal(snapshot?.entries[0]?.content, "[cancelled] Old step");
  assert.equal(snapshot?.entries[0]?.status, "completed");
});

test("parseTodoToolSnapshot maps done booleans and does not invent in_progress", () => {
  const snapshot = parseTodoToolSnapshot(
    {
      sessionUpdate: "tool_call",
      title: "todo",
      toolName: "todo",
      rawInput: {
        todos: [
          { text: "Inventory seams", done: true },
          { text: "Confirm payloads", done: false },
        ],
      },
    },
    META,
  );
  assert.equal(snapshot?.source, "todo-tool");
  assert.deepEqual(snapshot?.entries, [
    { content: "Inventory seams", status: "completed" },
    { content: "Confirm payloads", status: "pending" },
  ]);
});

test("parseTodoToolSnapshot preserves declared in_progress on TodoWrite rows", () => {
  const snapshot = parseTodoToolSnapshot(
    {
      sessionUpdate: "tool_call",
      title: "TodoWrite",
      rawInput: {
        todos: [
          { content: "Draft the note", status: "in_progress" },
          { content: "Ship it", status: "pending" },
        ],
      },
    },
    META,
  );
  assert.deepEqual(snapshot?.entries, [
    { content: "Draft the note", status: "in_progress" },
    { content: "Ship it", status: "pending" },
  ]);
});

test("snapshotFromObserverEvent clears on empty ACP entries", () => {
  const parsed = snapshotFromObserverEvent({
    seq: 9,
    timestamp: META.updatedAt,
    kind: "acp_read",
    agentIndex: 0,
    channelId: "chan",
    conversationId: "conv",
    sessionId: "sess-1",
    turnId: "turn-1",
    payload: {
      method: "session/update",
      params: {
        sessionId: "sess-1",
        update: { sessionUpdate: "plan", entries: [] },
      },
    },
  });
  assert.equal(parsed, "clear");
});

test("snapshotFromObserverEvent ignores assistant markdown", () => {
  const parsed = snapshotFromObserverEvent({
    seq: 9,
    timestamp: META.updatedAt,
    kind: "acp_read",
    agentIndex: 0,
    channelId: "chan",
    conversationId: "conv",
    sessionId: "sess-1",
    turnId: "turn-1",
    payload: {
      method: "session/update",
      params: {
        update: {
          sessionUpdate: "agent_message_chunk",
          content: { type: "text", text: "- [ ] Not a declared plan" },
        },
      },
    },
  });
  assert.equal(parsed, null);
});
