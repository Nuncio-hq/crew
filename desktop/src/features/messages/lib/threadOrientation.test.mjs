import assert from "node:assert/strict";
import test from "node:test";

import {
  buildMessageSnippet,
  buildThreadBreadcrumb,
} from "./threadOrientation.ts";

function message(overrides) {
  return {
    id: "message",
    renderKey: undefined,
    createdAt: 1_700_000_000,
    pubkey: "author",
    author: "Author",
    avatarUrl: null,
    role: undefined,
    personaDisplayName: undefined,
    time: "12:00 PM",
    body: "body",
    parentId: null,
    rootId: null,
    depth: 0,
    ...overrides,
  };
}

test("depth-0 head → one segment, anchor is the head", () => {
  const head = message({ id: "root", author: "Alice", body: "Hello thread" });
  const result = buildThreadBreadcrumb({
    channelName: "general",
    threadHead: head,
    messageById: new Map([[head.id, head]]),
  });
  assert.ok(result);
  assert.equal(result.channelName, "general");
  assert.equal(result.segments.length, 1);
  assert.equal(result.segments[0].author, "Alice");
  assert.equal(result.anchorMessageId, "root");
  assert.equal(result.truncated, false);
});

test("depth-1 head with parent present → two segments, anchor is parent", () => {
  const root = message({ id: "root", author: "Alice", body: "Root" });
  const head = message({
    id: "reply",
    author: "Bob",
    body: "Reply",
    parentId: "root",
    rootId: "root",
    depth: 1,
  });
  const result = buildThreadBreadcrumb({
    channelName: "general",
    threadHead: head,
    messageById: new Map([
      [root.id, root],
      [head.id, head],
    ]),
  });
  assert.ok(result);
  assert.equal(result.segments.length, 2);
  assert.equal(result.segments[0].author, "Alice");
  assert.equal(result.segments[1].author, "Bob");
  assert.equal(result.anchorMessageId, "root");
});

test("depth-2 head → three segments, anchor is top-level", () => {
  const root = message({ id: "root", author: "Alice", body: "Root" });
  const mid = message({
    id: "mid",
    author: "Bob",
    body: "Mid",
    parentId: "root",
    rootId: "root",
    depth: 1,
  });
  const head = message({
    id: "deep",
    author: "Carol",
    body: "Deep",
    parentId: "mid",
    rootId: "root",
    depth: 2,
  });
  const result = buildThreadBreadcrumb({
    channelName: "eng",
    threadHead: head,
    messageById: new Map([
      [root.id, root],
      [mid.id, mid],
      [head.id, head],
    ]),
  });
  assert.ok(result);
  assert.equal(result.segments.length, 3);
  assert.equal(result.anchorMessageId, "root");
  assert.equal(result.segments[0].author, "Alice");
  assert.equal(result.segments[2].author, "Carol");
});

test("depth-2 head whose parent is missing → walk breaks, anchor falls back to rootId", () => {
  const head = message({
    id: "deep",
    author: "Carol",
    body: "Deep",
    parentId: "missing",
    rootId: "root",
    depth: 2,
  });
  const result = buildThreadBreadcrumb({
    channelName: "general",
    threadHead: head,
    messageById: new Map([[head.id, head]]),
  });
  assert.ok(result);
  assert.equal(result.segments.length, 1);
  assert.equal(result.segments[0].author, "Carol");
  assert.equal(result.anchorMessageId, "root");
  assert.equal(result.anchorMessage, null);
});

test("normalized nested head (depth 0 + parentId) still walks to the root", () => {
  // MessageThreadPanel normalizes every head to depth 0.
  const root = message({ id: "root", author: "Alice", body: "Root" });
  const head = message({
    id: "reply",
    author: "Bob",
    body: "Reply",
    parentId: "root",
    rootId: "root",
    depth: 0,
  });
  const result = buildThreadBreadcrumb({
    channelName: "general",
    threadHead: head,
    messageById: new Map([
      [root.id, root],
      [head.id, head],
    ]),
  });
  assert.ok(result);
  assert.equal(result.segments.length, 2);
  assert.equal(result.anchorMessageId, "root");
  assert.equal(result.segments[0].author, "Alice");
});

test("chain of 5 → segments.length === 3, truncated, first is still top-level", () => {
  const ids = ["a", "b", "c", "d", "e"];
  const chain = ids.map((id, index) =>
    message({
      id,
      author: `Author${id}`,
      body: `Body ${id}`,
      parentId: index === 0 ? null : ids[index - 1],
      rootId: "a",
      depth: index,
    }),
  );
  const messageById = new Map(chain.map((m) => [m.id, m]));
  const result = buildThreadBreadcrumb({
    channelName: "general",
    threadHead: chain[4],
    messageById,
  });
  assert.ok(result);
  assert.equal(result.segments.length, 3);
  assert.equal(result.truncated, true);
  assert.equal(result.segments[0].message.id, "a");
  assert.equal(result.segments[1].message.id, "d");
  assert.equal(result.segments[2].message.id, "e");
  assert.equal(result.anchorMessageId, "a");
});

test("parent cycle terminates without hanging", () => {
  const a = message({
    id: "a",
    author: "A",
    parentId: "b",
    rootId: "root",
    depth: 1,
  });
  const b = message({
    id: "b",
    author: "B",
    parentId: "a",
    rootId: "root",
    depth: 1,
  });
  const result = buildThreadBreadcrumb({
    channelName: "general",
    threadHead: a,
    messageById: new Map([
      [a.id, a],
      [b.id, b],
    ]),
  });
  assert.ok(result);
  assert.ok(result.segments.length >= 1);
  assert.ok(result.segments.length <= 3);
});

test("snippet: newline collapsing, fenced-code stripping, truncation, empty → null", () => {
  assert.equal(buildMessageSnippet("hello\n\nworld"), "hello world");
  assert.equal(
    buildMessageSnippet("before\n```js\nconst x = 1;\n```\nafter"),
    "before after",
  );
  assert.equal(buildMessageSnippet(""), null);
  assert.equal(buildMessageSnippet("   "), null);
  assert.equal(buildMessageSnippet("```\nonly code\n```"), null);

  const long =
    "one two three four five six seven eight nine ten eleven twelve thirteen";
  const snip = buildMessageSnippet(long);
  assert.ok(snip);
  assert.ok(snip.endsWith("…"));
  assert.ok(snip.length <= 41);
});

test("every segment carries a snippet (phase 3 ancestry strip)", () => {
  const root = message({ id: "root", author: "Alice", body: "Root body" });
  const head = message({
    id: "reply",
    author: "Bob",
    body: "Reply body",
    parentId: "root",
    rootId: "root",
    depth: 1,
  });
  const result = buildThreadBreadcrumb({
    channelName: "general",
    threadHead: head,
    messageById: new Map([
      [root.id, root],
      [head.id, head],
    ]),
  });
  assert.ok(result);
  assert.equal(result.segments[0].snippet, "Root body");
  assert.equal(result.segments[1].snippet, "Reply body");
});

test("channelName empty → null", () => {
  const head = message({ id: "root" });
  assert.equal(
    buildThreadBreadcrumb({
      channelName: "",
      threadHead: head,
      messageById: new Map([[head.id, head]]),
    }),
    null,
  );
  assert.equal(
    buildThreadBreadcrumb({
      channelName: "   ",
      threadHead: head,
      messageById: new Map([[head.id, head]]),
    }),
    null,
  );
  assert.equal(
    buildThreadBreadcrumb({
      channelName: "general",
      threadHead: null,
      messageById: new Map(),
    }),
    null,
  );
});
