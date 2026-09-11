import assert from "node:assert/strict";
import { after, beforeEach, test } from "node:test";

const OWNER = "a".repeat(64);
const FOREIGN_OWNER = "b".repeat(64);
const REPO = "crew";
const COMMUNITY = "https://relay.example";
const SNAPSHOT = "12345678-1234-4234-9234-123456789abc";
const COORDINATE = `30617:${OWNER}:${REPO}`;
const SOURCE_REVISION = `git:${"c".repeat(40)}`;

const expectedScope = {
  scope: { owner: OWNER, community: COMMUNITY },
  workspace_generation: 3,
  identity_generation: 7,
};

let calls;

function event({
  id = "1".repeat(64),
  owner = OWNER,
  slug = "intro",
  content = "Canonical body text with Unicode café.",
  title = "Introduction",
} = {}) {
  return {
    id,
    pubkey: owner,
    kind: 30623,
    content,
    created_at: 10,
    sig: "",
    tags: [
      ["d", `${REPO}/${slug}`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT],
      ["title", title],
      ["section", "overview"],
      ["commit", "c".repeat(40)],
    ],
  };
}

function snapshot(pages = [event()]) {
  return {
    state: "complete",
    head: {
      ...event({ id: "2".repeat(64), slug: "_toc", content: "{}" }),
      tags: [
        ["d", `${REPO}/_toc`],
        ["a", COORDINATE],
        ["wiki-version", "1"],
        ["wiki-snapshot", SNAPSHOT],
        ["commit", "c".repeat(40)],
      ],
    },
    manifest: {
      ...event({ id: "3".repeat(64), slug: "m1-hash", content: "" }),
      content: JSON.stringify([
        1,
        SNAPSHOT,
        OWNER,
        REPO,
        SOURCE_REVISION,
        null,
        [],
        [],
      ]),
      tags: [
        ["d", `${REPO}/m1-hash`],
        ["a", COORDINATE],
        ["wiki-version", "1"],
        ["wiki-snapshot", SNAPSHOT],
      ],
    },
    pages,
    repoState: null,
  };
}

function installInvoke(handler) {
  globalThis.window ??= {};
  window.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      calls.push({ command, args });
      return handler(command, args);
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__;
}

beforeEach(() => {
  calls = [];
});

after(() => {
  delete globalThis.__TAURI_INTERNALS__;
  delete globalThis.window;
});

const {
  findWikiBodyMatch,
  searchWikiAtScope,
  verifiedWikiSnapshotIdentity,
  wikiSearchExcerpt,
} = await import("./wikiSearch.ts");

test("body match and excerpt are body-only, bounded, and Unicode-safe", () => {
  const content = "Title outside body\n\nCanonical café details are here.";
  const match = findWikiBodyMatch(content, "CAFÉ");
  assert.ok(match);
  assert.equal(match.text, "café");
  assert.match(wikiSearchExcerpt(content, match, 24), /café/);
  assert.equal(findWikiBodyMatch("Title only", "canonical"), null);
});

test("scoped search projects relay hits onto the verified snapshot and forwards NIP-50 args", async () => {
  const page = event();
  const graph = snapshot([page]);
  installInvoke((command) => {
    if (command === "wiki_search") {
      return {
        token: expectedScope,
        value: { events: [page], truncated: false },
      };
    }
    if (command === "owner_operation_scope") return expectedScope;
    throw new Error(`unexpected command ${command}`);
  });

  const result = await searchWikiAtScope({
    scope: expectedScope,
    coordinate: COORDINATE,
    snapshot: graph,
    query: "canonical",
  });
  assert.equal(result.snapshotId, SNAPSHOT);
  assert.equal(result.sourceRevision, SOURCE_REVISION);
  assert.equal(result.results[0].pageEventId, page.id);
  assert.equal(result.results[0].slug, "intro");
  assert.match(result.results[0].excerpt, /Canonical/);
  assert.deepEqual(calls[0], {
    command: "wiki_search",
    args: {
      expected: expectedScope,
      coordinate: COORDINATE,
      snapshotId: SNAPSHOT,
      query: "canonical",
    },
  });
});

test("old-generation and foreign-owner hits are rejected instead of becoming false matches", async () => {
  const page = event();
  const graph = snapshot([page]);
  installInvoke((command) => {
    if (command === "owner_operation_scope") return expectedScope;
    if (command === "wiki_search") {
      return {
        token: expectedScope,
        value: {
          events: [event({ id: "4".repeat(64) })],
          truncated: false,
        },
      };
    }
    throw new Error(`unexpected command ${command}`);
  });
  await assert.rejects(
    searchWikiAtScope({
      scope: expectedScope,
      coordinate: COORDINATE,
      snapshot: graph,
      query: "canonical",
    }),
    /old Wiki generation|outside the verified snapshot/,
  );

  installInvoke((command) => {
    if (command === "owner_operation_scope") return expectedScope;
    if (command === "wiki_search") {
      return {
        token: expectedScope,
        value: {
          events: [event({ id: "5".repeat(64), owner: FOREIGN_OWNER })],
          truncated: false,
        },
      };
    }
    throw new Error(`unexpected command ${command}`);
  });
  await assert.rejects(
    searchWikiAtScope({
      scope: expectedScope,
      coordinate: COORDINATE,
      snapshot: graph,
      query: "canonical",
    }),
    /outside the verified snapshot/,
  );
});

test("the native result cap stays visible when more than fifty verified pages match", async () => {
  const pages = Array.from({ length: 51 }, (_, index) =>
    event({
      id: index.toString(16).padStart(64, "0"),
      slug: `page-${index}`,
      content: `Canonical page ${index}`,
    }),
  );
  const graph = snapshot(pages);
  installInvoke((command) => {
    if (command === "owner_operation_scope") return expectedScope;
    if (command === "wiki_search") {
      return {
        token: expectedScope,
        value: { events: pages, truncated: true },
      };
    }
    throw new Error(`unexpected command ${command}`);
  });
  const result = await searchWikiAtScope({
    scope: expectedScope,
    coordinate: COORDINATE,
    snapshot: graph,
    query: "canonical",
  });
  assert.equal(result.results.length, 50);
  assert.equal(result.truncated, true);
});

test("inconsistent native truncation metadata fails closed", async () => {
  const page = event();
  const graph = snapshot([page]);
  installInvoke((command) => {
    if (command === "owner_operation_scope") return expectedScope;
    if (command === "wiki_search") {
      return {
        token: expectedScope,
        value: {
          events: Array.from({ length: 51 }, () => page),
          truncated: false,
        },
      };
    }
    throw new Error(`unexpected command ${command}`);
  });
  await assert.rejects(
    searchWikiAtScope({
      scope: expectedScope,
      coordinate: COORDINATE,
      snapshot: graph,
      query: "canonical",
    }),
    /unbounded result/,
  );
});

test("an incomplete snapshot never invokes a live unscoped search", async () => {
  installInvoke(() => {
    throw new Error("wiki_search must not be called");
  });
  assert.equal(
    verifiedWikiSnapshotIdentity(
      {
        state: "incomplete",
        head: null,
        manifest: null,
        pages: [],
        repoState: null,
      },
      COORDINATE,
    ),
    null,
  );
  await assert.rejects(
    searchWikiAtScope({
      scope: expectedScope,
      coordinate: COORDINATE,
      snapshot: {
        state: "legacy",
        head: null,
        manifest: null,
        pages: [],
        repoState: null,
      },
      query: "canonical",
    }),
    /complete verified snapshot/,
  );
  assert.deepEqual(calls, []);
});
