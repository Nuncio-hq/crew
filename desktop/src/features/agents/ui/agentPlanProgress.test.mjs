import assert from "node:assert/strict";
import test from "node:test";

import {
  getAgentPlanProgress,
  parseAgentPlanTodos,
} from "./agentPlanProgress.ts";
import { classifyTool } from "./agentSessionToolClassifier.ts";

test("parseAgentPlanTodos keeps the full {text, done} array", () => {
  assert.deepEqual(
    parseAgentPlanTodos({
      todos: [
        { text: "Read the request", done: true },
        { text: "Wire reply destination", done: false },
        { text: "Report back", done: false },
      ],
    }),
    [
      { text: "Read the request", done: true },
      { text: "Wire reply destination", done: false },
      { text: "Report back", done: false },
    ],
  );
});

test("parseAgentPlanTodos returns null for empty, missing, or malformed shapes", () => {
  assert.equal(parseAgentPlanTodos({}), null);
  assert.equal(parseAgentPlanTodos({ todos: [] }), null);
  assert.equal(parseAgentPlanTodos({ todos: "nope" }), null);
  assert.equal(parseAgentPlanTodos({ todos: [{ done: false }] }), null);
  assert.equal(parseAgentPlanTodos({ todos: [null] }), null);
});

test("classifyTool preserves todos beside the preview string", () => {
  const todos = [
    { text: "Ship compact summaries", done: false },
    { text: "Verify UI", done: true },
  ];
  const descriptor = classifyTool({
    title: "todo",
    toolName: "todo",
    buzzToolName: null,
    args: { todos },
    result: "",
    isError: false,
  });

  assert.equal(descriptor.renderClass, "plan");
  assert.equal(descriptor.preview, "Ship compact summaries (+1)");
  assert.deepEqual(descriptor.todos, [
    { text: "Ship compact summaries", done: false },
    { text: "Verify UI", done: true },
  ]);
});

test("getAgentPlanProgress prefers the newest todo snapshot", () => {
  const progress = getAgentPlanProgress([
    {
      id: "t1",
      type: "tool",
      renderClass: "plan",
      title: "Updated todos",
      toolName: "todo",
      buzzToolName: null,
      status: "completed",
      args: {
        todos: [
          { text: "Old step", done: false },
          { text: "Also old", done: false },
        ],
      },
      result: "",
      isError: false,
      timestamp: "2026-08-02T00:00:00.000Z",
      startedAt: "2026-08-02T00:00:00.000Z",
      completedAt: "2026-08-02T00:00:01.000Z",
      descriptor: {
        renderClass: "plan",
        label: "Updated todos",
        preview: "Old step (+1)",
        groupKey: "plan:todo",
        todos: [
          { text: "Old step", done: false },
          { text: "Also old", done: false },
        ],
      },
      channelId: "chan-1",
    },
    {
      id: "t2",
      type: "tool",
      renderClass: "plan",
      title: "Updated todos",
      toolName: "todo",
      buzzToolName: null,
      status: "completed",
      args: {
        todos: [
          { text: "Read the request", done: true },
          { text: "Wire reply destination", done: false },
        ],
      },
      result: "",
      isError: false,
      timestamp: "2026-08-02T00:01:00.000Z",
      startedAt: "2026-08-02T00:01:00.000Z",
      completedAt: "2026-08-02T00:01:01.000Z",
      descriptor: {
        renderClass: "plan",
        label: "Updated todos",
        preview: "Read the request (+1)",
        groupKey: "plan:todo",
        todos: [
          { text: "Read the request", done: true },
          { text: "Wire reply destination", done: false },
        ],
      },
      channelId: "chan-1",
    },
  ]);

  assert.deepEqual(progress, {
    steps: [
      { text: "Read the request", done: true },
      { text: "Wire reply destination", done: false },
    ],
    currentIndex: 1,
    updatedAt: "2026-08-02T00:01:00.000Z",
  });
});

test("getAgentPlanProgress returns null when no checklist is available", () => {
  assert.equal(
    getAgentPlanProgress([
      {
        id: "m1",
        type: "message",
        renderClass: "message",
        role: "assistant",
        title: "Reply",
        text: "hello",
        timestamp: "2026-08-02T00:00:00.000Z",
      },
    ]),
    null,
  );
});
