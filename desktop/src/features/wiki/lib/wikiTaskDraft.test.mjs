/**
 * #367 — private Wiki task drafts.
 *
 * The draft is private editable user state held in the shared scoped draft
 * store under an explicit `wiki:` namespace: saving publishes nothing, a
 * title-only draft (no prompt text yet) must survive, and Wiki saves are
 * partitioned from channel composer drafts so neither side silently evicts
 * the other.
 */

import assert from "node:assert/strict";
import test from "node:test";

function makeLocalStorage() {
  const store = new Map();
  return {
    get length() {
      return store.size;
    },
    key: (i) => [...store.keys()][i] ?? null,
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, value),
    removeItem: (key) => store.delete(key),
    clear: () => store.clear(),
  };
}

function installFreshLocalStorage() {
  const ls = makeLocalStorage();
  if (typeof globalThis.window === "undefined") {
    globalThis.window = { localStorage: ls };
  } else {
    globalThis.window.localStorage = ls;
  }
  Object.defineProperty(globalThis, "localStorage", {
    get: () => globalThis.window.localStorage,
    configurable: true,
  });
  return ls;
}

installFreshLocalStorage();

const { clearAllDrafts, initDraftStore, loadDraftEntry, saveDraftEntry } =
  await import("../../messages/lib/useDrafts.ts");
const {
  WIKI_TASK_DRAFT_MAX,
  clearWikiTaskDraft,
  loadWikiTaskDraft,
  newWikiTaskDraftMeta,
  saveWikiTaskDraft,
  wikiTaskDraftKey,
} = await import("./wikiTaskDraft.ts");

const OWNER = "a".repeat(64);
const AGENT = "b".repeat(64);
const COORDINATE = `${OWNER}:crew`;
const OTHER_COORDINATE = `${OWNER}:other-repo`;
const QUESTION_ID = "11111111-1111-4111-8111-111111111111";
const ATTEMPT_ID = "22222222-2222-4222-8222-222222222222";

const DRAFT_INPUT = {
  questionId: QUESTION_ID,
  attemptId: ATTEMPT_ID,
  question: "How does dispatch work?",
  markdown: "It builds a signed kind-9 event and posts it.",
  citations: [
    { path: "src/relay.rs", startLine: 10, endLine: 20 },
    { path: "src/events.rs", startLine: 200, endLine: 210 },
  ],
  manifest: null,
  sourceRevision: "git:abc123",
  originCoordinate: COORDINATE,
  originAgent: AGENT,
};

function setup(pubkey = "pubkey-owner", relayUrl = "ws://relay-a") {
  installFreshLocalStorage();
  clearAllDrafts();
  initDraftStore(pubkey, relayUrl);
}

function composerDraft(channelId, content, updatedAt) {
  const now = updatedAt ?? new Date().toISOString();
  return {
    content,
    selectionStart: content.length,
    selectionEnd: content.length,
    channelId,
    createdAt: now,
    updatedAt: now,
    pendingImeta: [],
    spoileredAttachmentUrls: [],
  };
}

function scope(overrides = {}) {
  return {
    projectId: "proj-1",
    repositoryCoordinate: COORDINATE,
    attemptId: ATTEMPT_ID,
    ...overrides,
  };
}

// ── key scoping ─────────────────────────────────────────────────────────────

test("wiki_task_draft_key_uses_explicit_wiki_namespace_and_full_scope", () => {
  const key = wikiTaskDraftKey(scope());
  assert.ok(key.startsWith("wiki:task:"), `expected wiki: namespace: ${key}`);
  assert.ok(key.includes("proj-1"));
  assert.ok(key.includes(COORDINATE));
  assert.ok(key.includes(ATTEMPT_ID));
  // Different attempts/projects/repos must produce distinct drafts.
  assert.notEqual(
    wikiTaskDraftKey(
      scope({ attemptId: "33333333-3333-4333-8333-333333333333" }),
    ),
    key,
  );
  assert.notEqual(wikiTaskDraftKey(scope({ projectId: "proj-2" })), key);
  assert.notEqual(
    wikiTaskDraftKey(scope({ repositoryCoordinate: OTHER_COORDINATE })),
    key,
  );
  // A wiki key can never collide with a channel or thread composer key.
  assert.notEqual(wikiTaskDraftKey(scope()), "channel-1");
  assert.notEqual(wikiTaskDraftKey(scope()), `thread:${ATTEMPT_ID}`);
});

// ── meta construction ───────────────────────────────────────────────────────

test("new_meta_carries_origin_and_includes_all_citations_by_default", () => {
  const meta = newWikiTaskDraftMeta(DRAFT_INPUT);
  assert.equal(meta.origin.questionId, QUESTION_ID);
  assert.equal(meta.origin.attemptId, ATTEMPT_ID);
  assert.equal(meta.origin.coordinate, COORDINATE);
  assert.equal(meta.origin.sourceRevision, "git:abc123");
  assert.equal(meta.origin.originAgent, AGENT);
  assert.equal(meta.references.length, 2);
  assert.ok(meta.references.every((ref) => ref.included));
  assert.equal(meta.agentPubkey, null);
  assert.equal(meta.dispatchId, null);
});

// ── save / reopen ───────────────────────────────────────────────────────────

test("save_then_reopen_restores_title_prompt_channel_agent_and_references", () => {
  setup();
  const key = wikiTaskDraftKey(scope());
  const meta = newWikiTaskDraftMeta(DRAFT_INPUT);
  meta.title = "Wire the dispatch seam";
  meta.agentPubkey = AGENT;
  meta.references[1].included = false;
  saveWikiTaskDraft(key, meta, "Do the dispatch work", "channel-9");

  const restored = loadWikiTaskDraft(key);
  assert.ok(restored, "draft must persist");
  assert.equal(restored.meta.title, "Wire the dispatch seam");
  assert.equal(restored.meta.agentPubkey, AGENT);
  assert.equal(restored.prompt, "Do the dispatch work");
  assert.equal(restored.channelId, "channel-9");
  assert.equal(restored.meta.references[0].included, true);
  assert.equal(restored.meta.references[1].included, false);
  assert.equal(restored.meta.origin.attemptId, ATTEMPT_ID);
});

test("draft_survives_store_reinit_like_an_app_restart", () => {
  setup();
  const key = wikiTaskDraftKey(scope());
  saveWikiTaskDraft(
    key,
    newWikiTaskDraftMeta(DRAFT_INPUT),
    "prompt text",
    "channel-9",
  );
  // Simulate restart: drop the in-memory cache, rebind the same identity.
  clearAllDrafts();
  initDraftStore("pubkey-owner", "ws://relay-a");
  const restored = loadWikiTaskDraft(key);
  assert.ok(restored, "draft must survive re-init");
  assert.equal(restored.prompt, "prompt text");
});

test("title_only_draft_with_empty_prompt_persists", () => {
  setup();
  const key = wikiTaskDraftKey(scope());
  const meta = newWikiTaskDraftMeta(DRAFT_INPUT);
  meta.title = "Only a title so far";
  saveWikiTaskDraft(key, meta, "", "channel-9");
  const restored = loadWikiTaskDraft(key);
  assert.ok(
    restored,
    "a saved wiki draft with no prompt text must not be dropped",
  );
  assert.equal(restored.meta.title, "Only a title so far");
  assert.equal(restored.prompt, "");
});

// ── isolation from unrelated drafts ─────────────────────────────────────────

test("saving_a_wiki_draft_never_touches_an_unrelated_channel_draft", () => {
  setup();
  saveDraftEntry("channel-1", composerDraft("channel-1", "unrelated draft"));
  const key = wikiTaskDraftKey(scope());
  saveWikiTaskDraft(
    key,
    newWikiTaskDraftMeta(DRAFT_INPUT),
    "wiki prompt",
    "channel-1",
  );
  const unrelated = loadDraftEntry("channel-1");
  assert.equal(
    unrelated?.content,
    "unrelated draft",
    "the channel composer draft must be preserved verbatim",
  );
});

test("wiki_drafts_have_their_own_eviction_partition", () => {
  setup();
  const key = wikiTaskDraftKey(scope());
  const old = new Date(1_000_000).toISOString();
  // Make the wiki draft the oldest-updated entry by far.
  saveWikiTaskDraft(
    key,
    newWikiTaskDraftMeta(DRAFT_INPUT),
    "important wiki task",
    "channel-1",
    { updatedAt: old },
  );
  // Flood the shared composer partition past its 100-entry cap.
  for (let i = 0; i <= 110; i++) {
    const ts = new Date(2_000_000 + i * 1000).toISOString();
    saveDraftEntry(`chan-${i}`, composerDraft(`chan-${i}`, `draft ${i}`, ts));
  }
  const restored = loadWikiTaskDraft(key);
  assert.ok(
    restored,
    "composer eviction pressure must not evict a wiki task draft",
  );
  assert.equal(restored.prompt, "important wiki task");
});

test("wiki_partition_is_bounded_and_evicts_its_own_oldest", () => {
  setup();
  const base = Date.parse("2026-01-01T00:00:00.000Z");
  for (let i = 0; i <= WIKI_TASK_DRAFT_MAX; i++) {
    const ts = new Date(base + i * 1000).toISOString();
    saveWikiTaskDraft(
      wikiTaskDraftKey(scope({ attemptId: `${i}-attempt` })),
      newWikiTaskDraftMeta({
        ...DRAFT_INPUT,
        attemptId: `${i}-attempt`,
      }),
      `prompt ${i}`,
      "channel-1",
      { updatedAt: ts },
    );
  }
  assert.equal(
    loadWikiTaskDraft(wikiTaskDraftKey(scope({ attemptId: "0-attempt" }))),
    null,
    "oldest wiki draft must evict inside its own partition",
  );
  assert.ok(
    loadWikiTaskDraft(
      wikiTaskDraftKey(scope({ attemptId: `${WIKI_TASK_DRAFT_MAX}-attempt` })),
    ),
    "newest wiki draft survives",
  );
  // A channel draft written alongside is untouched by wiki-partition pressure.
  saveDraftEntry("channel-alive", composerDraft("channel-alive", "still here"));
  assert.equal(loadDraftEntry("channel-alive")?.content, "still here");
});

// ── cancel semantics ────────────────────────────────────────────────────────

test("clearing_only_removes_the_wiki_draft", () => {
  setup();
  saveDraftEntry("channel-1", composerDraft("channel-1", "keep me"));
  const key = wikiTaskDraftKey(scope());
  saveWikiTaskDraft(
    key,
    newWikiTaskDraftMeta(DRAFT_INPUT),
    "prompt",
    "channel-1",
  );
  clearWikiTaskDraft(key);
  assert.equal(loadWikiTaskDraft(key), null);
  assert.equal(loadDraftEntry("channel-1")?.content, "keep me");
});

test("viewer_and_community_scoping_isolate_drafts", () => {
  setup("pubkey-owner", "ws://relay-a");
  const key = wikiTaskDraftKey(scope());
  saveWikiTaskDraft(key, newWikiTaskDraftMeta(DRAFT_INPUT), "a", "c");
  // Same attempt under a different community relay: no draft.
  clearAllDrafts();
  initDraftStore("pubkey-owner", "ws://relay-b");
  assert.equal(loadWikiTaskDraft(key), null);
  // Same community under a different viewer: no draft.
  clearAllDrafts();
  initDraftStore("pubkey-other", "ws://relay-a");
  assert.equal(loadWikiTaskDraft(key), null);
  // Back to the original scope: the draft returns.
  clearAllDrafts();
  initDraftStore("pubkey-owner", "ws://relay-a");
  assert.ok(loadWikiTaskDraft(key));
});

// ── corruption tolerance ────────────────────────────────────────────────────

test("a_corrupt_meta_blob_reads_as_no_wiki_draft_without_throwing", () => {
  setup();
  const key = wikiTaskDraftKey(scope());
  const now = new Date().toISOString();
  saveDraftEntry(key, {
    content: "x",
    selectionStart: 0,
    selectionEnd: 0,
    channelId: "channel-1",
    createdAt: now,
    updatedAt: now,
    pendingImeta: [],
    spoileredAttachmentUrls: [],
    meta: { kind: "something-else" },
  });
  assert.equal(loadWikiTaskDraft(key), null);
});
