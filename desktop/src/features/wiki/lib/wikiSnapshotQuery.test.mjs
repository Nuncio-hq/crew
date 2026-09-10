import assert from "node:assert/strict";
import test from "node:test";
import { fixture } from "./wikiSnapshotV1.fixtures.mjs";
import { readWikiSnapshot } from "./wikiSnapshotQuery.ts";

function reader(input, override) {
  const calls = [];
  const events = [input.head, input.manifest, ...input.pages];
  return {
    calls,
    isCurrent: () => true,
    query: async (filter) => {
      calls.push(filter);
      if (override) return override(filter, events);
      return events.filter((event) =>
        filter.ids
          ? filter.ids.includes(event.id)
          : event.tags.some(
              (tag) => tag[0] === "d" && filter["#d"].includes(tag[1]),
            ),
      );
    },
  };
}

test("fetches exact owner/repository head, manifest and pages without global samples", async () => {
  const input = fixture();
  const transport = reader(input);
  const result = await readWikiSnapshot(input, transport);
  assert.equal(result.status, "verified");
  assert.equal(transport.calls.length, 3);
  for (const call of transport.calls) {
    assert.deepEqual(call.kinds, [30623]);
    assert.deepEqual(call.authors, [input.owner]);
    assert.deepEqual(call["#a"], [`30617:${input.owner}:${input.repoD}`]);
    assert.ok(call["#d"].length > 0 && call["#d"].length <= 64);
    assert.ok(call.limit <= 64);
  }
  assert.deepEqual(transport.calls[0]["#d"], [`${input.repoD}/_toc`]);
  assert.deepEqual(transport.calls[1].ids, [input.manifest.id]);
  assert.deepEqual(transport.calls[2].ids, [input.pages[0].id]);
});

test("missing member is incomplete and never a successful empty snapshot", async () => {
  const input = fixture();
  const transport = reader(input, (filter, events) =>
    events.filter(
      (e) =>
        e.id !== input.pages[0].id &&
        (filter.ids ? filter.ids.includes(e.id) : e.id === input.head.id),
    ),
  );
  assert.deepEqual(await readWikiSnapshot(input, transport), {
    status: "incomplete",
    reason: "invalid",
  });
});

test("a failed relay read is unavailable, not absent", async () => {
  const input = fixture();
  const transport = reader(input, () => {
    throw new Error("offline");
  });
  assert.deepEqual(await readWikiSnapshot(input, transport), {
    status: "incomplete",
    reason: "unavailable",
  });
});

test("an empty scoped head response is absent", async () => {
  const input = fixture();
  const transport = reader(input, () => []);
  assert.deepEqual(await readWikiSnapshot(input, transport), {
    status: "absent",
  });
  assert.equal(transport.calls.length, 1);
});

test("scope retirement after first response prevents further requests", async () => {
  const input = fixture();
  const transport = reader(input);
  transport.isCurrent = () => transport.calls.length === 0;
  assert.deepEqual(await readWikiSnapshot(input, transport), {
    status: "incomplete",
    reason: "interrupted",
  });
  assert.equal(transport.calls.length, 1);
});

test("two returned heads fail closed instead of picking arbitrary authority", async () => {
  const input = fixture();
  const transport = reader(input, () => [input.head, input.head]);
  assert.deepEqual(await readWikiSnapshot(input, transport), {
    status: "incomplete",
    reason: "invalid",
  });
  assert.equal(transport.calls.length, 1);
});

test("65 signed members use batches of 64 and 1", async () => {
  const { multiFixture } = await import("./wikiSnapshotV1.fixtures.mjs");
  const input = multiFixture(65);
  const transport = reader(input);
  const result = await readWikiSnapshot(input, transport);
  assert.equal(result.status, "verified");
  assert.equal(result.snapshot.pages.length, 65);
  assert.deepEqual(
    transport.calls.map((call) => call.limit),
    [2, 1, 64, 1],
  );
  assert.deepEqual(
    transport.calls.slice(2).flatMap((call) => call.ids),
    input.pages.map((page) => page.id),
  );
});

test("an already retired scope sends no request", async () => {
  const input = fixture();
  const transport = reader(input);
  transport.isCurrent = () => false;
  assert.deepEqual(await readWikiSnapshot(input, transport), {
    status: "incomplete",
    reason: "interrupted",
  });
  assert.equal(transport.calls.length, 0);
});

test("scope retirement after an empty response cannot become authoritative absence", async () => {
  const input = fixture();
  const transport = reader(input, () => []);
  transport.isCurrent = () => transport.calls.length === 0;
  assert.deepEqual(await readWikiSnapshot(input, transport), {
    status: "incomplete",
    reason: "interrupted",
  });
  assert.equal(transport.calls.length, 1);
});
