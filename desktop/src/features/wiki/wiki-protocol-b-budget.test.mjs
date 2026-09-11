import assert from "node:assert/strict";
import { test } from "node:test";

import {
  BudgetExceededError,
  MAX_DESCRIPTOR_BYTES,
  MAX_EVENT_BYTES,
  MAX_GRAPH_BYTES,
  MAX_PUBLICATION_PAGES,
  MAX_RESPONSE_BYTES,
  MAX_RESPONSE_EVENTS,
  PAGE_QUERY_BATCH,
  assertFilterBudget,
  createGraphBudget,
  createResponseCollector,
  readBoundedJson,
  responseEventCap,
} from "./wiki-protocol-b-budget.mjs";

// These bind the live Protocol B harness to its own budgets without a relay.
// The live acceptance test imports the same module, so a bound that regresses
// here regresses there.

function fakeEvent(bytes, id = "e") {
  // JSON.stringify of {id, content} adds a fixed frame; oversize by padding
  // content, which is the field a hostile relay would actually grow.
  return { id, content: "x".repeat(Math.max(0, bytes)) };
}

test("published bounds are the documented Protocol B budgets", () => {
  assert.equal(MAX_EVENT_BYTES, 192 * 1024);
  assert.equal(MAX_RESPONSE_BYTES, 1024 * 1024);
  assert.equal(MAX_GRAPH_BYTES, 64 * 1024 * 1024);
  assert.equal(MAX_PUBLICATION_PAGES, 256);
  assert.equal(PAGE_QUERY_BATCH, 4);
  assert.equal(MAX_RESPONSE_EVENTS, PAGE_QUERY_BATCH + 1);
  assert.equal(MAX_DESCRIPTOR_BYTES, 64 * 1024);
});

test("responseEventCap never exceeds the absolute retained-event ceiling", () => {
  assert.equal(responseEventCap({ limit: 2 }), 2);
  assert.equal(
    responseEventCap({ limit: PAGE_QUERY_BATCH + 1 }),
    MAX_RESPONSE_EVENTS,
  );
  assert.equal(responseEventCap({ limit: 10_000 }), MAX_RESPONSE_EVENTS);
  assert.equal(responseEventCap({}), MAX_RESPONSE_EVENTS);
  assert.equal(responseEventCap({ limit: 0 }), 1);
});

test("an oversized event is rejected before it is retained", () => {
  const collector = createResponseCollector({ maxEvents: 4 });
  assert.throws(
    () => collector.admit(fakeEvent(MAX_EVENT_BYTES + 1)),
    BudgetExceededError,
  );
  assert.equal(collector.events.length, 0);
  assert.equal(collector.bytes, 0);
});

test("the retained-event count is bounded per subscription", () => {
  const collector = createResponseCollector({ maxEvents: 2 });
  collector.admit(fakeEvent(16, "a"));
  collector.admit(fakeEvent(16, "b"));
  assert.throws(() => collector.admit(fakeEvent(16, "c")), BudgetExceededError);
  assert.deepEqual(
    collector.events.map((event) => event.id),
    ["a", "b"],
  );
});

test("aggregate response bytes are bounded before retention", () => {
  const collector = createResponseCollector({ maxEvents: 64 });
  const each = 190 * 1024;
  let admitted = 0;
  assert.throws(() => {
    for (let index = 0; index < 16; index += 1) {
      collector.admit(fakeEvent(each, `e${index}`));
      admitted += 1;
    }
  }, BudgetExceededError);
  assert.equal(collector.events.length, admitted);
  assert.ok(
    collector.bytes <= MAX_RESPONSE_BYTES,
    "retained bytes must never cross the response budget",
  );
});

test("the graph budget accumulates across subscriptions", () => {
  const graph = createGraphBudget({ maxBytes: 400 });
  const head = createResponseCollector({ maxEvents: 4, graph });
  head.admit(fakeEvent(100, "head"));
  const pages = createResponseCollector({ maxEvents: 4, graph });
  pages.admit(fakeEvent(100, "page-1"));
  assert.equal(graph.events, 2);
  assert.ok(graph.bytes > 200 && graph.bytes <= 400);
  assert.throws(
    () => pages.admit(fakeEvent(300, "page-2")),
    BudgetExceededError,
  );
  assert.equal(pages.events.length, 1);
  assert.equal(graph.events, 2);
  assert.equal(createGraphBudget().maxBytes, MAX_GRAPH_BYTES);
  assert.equal(createGraphBudget().maxPages, MAX_PUBLICATION_PAGES);
});

test("request filters are still measured on the way out", () => {
  assert.equal(typeof assertFilterBudget({ kinds: [30_623] }), "number");
  assert.throws(
    () => assertFilterBudget({ ids: ["a".repeat(2 * 1024 * 1024)] }),
    BudgetExceededError,
  );
});

test("readBoundedJson parses a small descriptor", async () => {
  const descriptor = {
    supported_extensions: ["crew-conditional-publication-v1"],
  };
  const value = await readBoundedJson(
    new Response(JSON.stringify(descriptor), {
      headers: { "content-type": "application/nostr+json" },
    }),
  );
  assert.deepEqual(value, descriptor);
});

test("readBoundedJson refuses an oversized declared body", async () => {
  const response = new Response("{}", {
    headers: { "content-length": String(MAX_DESCRIPTOR_BYTES + 1) },
  });
  await assert.rejects(() => readBoundedJson(response), BudgetExceededError);
});

test("readBoundedJson cancels a stream that outgrows the bound", async () => {
  let cancelled = false;
  const body = new ReadableStream({
    pull(controller) {
      controller.enqueue(new TextEncoder().encode("x".repeat(4096)));
    },
    cancel() {
      cancelled = true;
    },
  });
  // No content-length: the cap has to hold on the streamed bytes themselves.
  await assert.rejects(
    () => readBoundedJson(new Response(body), { maxBytes: 8 * 1024 }),
    BudgetExceededError,
  );
  assert.equal(cancelled, true);
});
