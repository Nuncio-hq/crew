/**
 * #367 — the editable private Wiki task draft and its explicit dispatch,
 * driven through the real panel component against the stubbed Tauri
 * boundary. Asserts the command sequence the native commands receive (the
 * durable kickoff seam), not a mocked callback.
 */
import assert from "node:assert/strict";
import { after, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  localStorage: dom.window.localStorage,
  window: dom.window,
});
if (!dom.window.crypto?.randomUUID) {
  dom.window.crypto = globalThis.crypto;
}

const OWNER = "a".repeat(64);
const AGENT = "b".repeat(64);
const OTHER_MEMBER = "c".repeat(64);
const COORDINATE = `${OWNER}:crew`;
const CHANNEL_ID = "33333333-3333-4333-8333-333333333333";
const QUESTION_ID = "11111111-1111-4111-8111-111111111111";
const ATTEMPT_ID = "22222222-2222-4222-8222-222222222222";
const EVENT_ID = "e".repeat(64);

const SCOPE = {
  scope: { owner: OWNER, community: "ws://relay" },
  workspace_generation: 1,
  identity_generation: 1,
};

const DRAFT = {
  questionId: QUESTION_ID,
  attemptId: ATTEMPT_ID,
  question: "How does dispatch work?\n\nmore detail",
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

let calls = [];
let submitResult = null;
let submitError = null;
let prepareError = null;
let membersList = [];

dom.window.__TAURI_INTERNALS__ = {
  transformCallback: (callback) => callback,
  invoke: async (command, payload) => {
    calls.push({ command, payload });
    switch (command) {
      case "get_channels":
        return {
          hash: "h",
          channels: [
            {
              id: CHANNEL_ID,
              name: "crew-dev",
              channel_type: "standard",
              visibility: "private",
              is_member: true,
            },
          ],
          last_messages: {},
        };
      case "get_channel_members":
        return {
          members: membersList.map((m) => ({
            pubkey: m.pubkey,
            role: m.role,
            is_agent: m.isAgent,
            joined_at: "2026-01-01T00:00:00Z",
            display_name: m.name,
          })),
          next_cursor: null,
        };
      case "owner_operation_scope":
        return SCOPE;
      case "wiki_task_dispatch_prepare":
        if (prepareError) throw new Error(prepareError);
        return {
          token: SCOPE,
          value: {
            dispatchId: payload.input.dispatchId,
            eventId: EVENT_ID,
            channelId: payload.input.channelId,
            rootEventId: null,
            status: "pending",
            accepted: false,
            submitAttempts: 0,
            error: null,
          },
        };
      case "wiki_task_dispatch_submit":
        if (submitError) throw new Error(submitError);
        return {
          token: SCOPE,
          value: {
            dispatchId: payload.dispatchId,
            eventId: EVENT_ID,
            channelId: CHANNEL_ID,
            rootEventId: submitResult?.accepted ? EVENT_ID : null,
            status: submitResult?.accepted ? "complete" : "failed",
            accepted: Boolean(submitResult?.accepted),
            submitAttempts: 1,
            error: submitResult?.accepted ? null : "relay unreachable",
          },
        };
      case "wiki_task_dispatch_reconcile":
        return { token: SCOPE, value: submitResult };
      case "wiki_task_dispatch_abandon":
        return { token: SCOPE, value: { status: "canceled", accepted: false } };
      default:
        throw new Error(`unexpected command ${command}`);
    }
  },
};

const { act, cleanup, fireEvent, render, screen } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { WikiTaskDispatchPanel } = await import(
  "@/features/wiki/ui/WikiTaskDispatchPanel"
);
const { initDraftStore, loadDraftEntry } = await import(
  "../../messages/lib/useDrafts.ts"
);
const { loadWikiTaskDraft, wikiTaskDraftKey } = await import(
  "../lib/wikiTaskDraft.ts"
);

const KEY = wikiTaskDraftKey({
  projectId: "proj-1",
  repositoryCoordinate: COORDINATE,
  attemptId: ATTEMPT_ID,
});

beforeEach(() => {
  cleanup();
  calls = [];
  submitResult = null;
  submitError = null;
  prepareError = null;
  membersList = [
    { pubkey: AGENT, role: "bot", isAgent: true, name: "Scout" },
    { pubkey: OTHER_MEMBER, role: "member", isAgent: false, name: "Oscar" },
  ];
  dom.window.localStorage.clear();
  initDraftStore(OWNER, "ws://relay");
});

after(() => {
  cleanup();
  dom.window.close();
});

function mount(overrides = {}) {
  const accepted = [];
  render(
    React.createElement(WikiTaskDispatchPanel, {
      draft: DRAFT,
      onAccepted: (channelId, rootId) => accepted.push({ channelId, rootId }),
      onClose: () => {},
      projectId: "proj-1",
      ...overrides,
    }),
  );
  return { accepted };
}

test("the panel seeds the draft fields and keeps Save local", async () => {
  mount();
  await act(async () => {});
  await act(async () => {});

  assert.equal(
    screen.getByTestId("wiki-task-title").value,
    "How does dispatch work?",
  );
  assert.equal(
    screen.getByTestId("wiki-task-prompt").value,
    "It builds a signed kind-9 event and posts it.",
  );
  // Both citations start checked.
  assert.equal(screen.getByTestId("wiki-task-ref-src/relay.rs").checked, true);
  assert.equal(screen.getByTestId("wiki-task-ref-src/events.rs").checked, true);

  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-task-save"));
  });
  const stored = loadWikiTaskDraft(KEY);
  assert.ok(stored, "the draft must persist");
  assert.equal(stored.meta.title, "How does dispatch work?");
  assert.equal(
    calls.filter((c) => c.command.startsWith("wiki_task_dispatch")).length,
    0,
    "Save must never touch the dispatch boundary",
  );
});

test("edits and channel/agent selection persist across a reopen", async () => {
  mount();
  await act(async () => {});
  await act(async () => {});

  await act(async () => {
    fireEvent.change(screen.getByTestId("wiki-task-title"), {
      target: { value: "Edited title" },
    });
    fireEvent.change(screen.getByTestId("wiki-task-channel"), {
      target: { value: CHANNEL_ID },
    });
  });
  await act(async () => {});
  await act(async () => {
    fireEvent.change(screen.getByTestId("wiki-task-agent"), {
      target: { value: AGENT },
    });
    fireEvent.click(screen.getByTestId("wiki-task-save"));
  });
  const stored = loadWikiTaskDraft(KEY);
  assert.equal(stored.meta.title, "Edited title");
  assert.equal(stored.meta.agentPubkey, AGENT);
  assert.equal(stored.channelId, CHANNEL_ID);

  // Reopen — fields restore from the private draft.
  cleanup();
  mount();
  await act(async () => {});
  assert.equal(screen.getByTestId("wiki-task-title").value, "Edited title");
  assert.equal(screen.getByTestId("wiki-task-channel").value, CHANNEL_ID);
  assert.equal(screen.getByTestId("wiki-task-agent").value, AGENT);
});

test("Start runs the durable prepare → submit sequence with only included refs", async () => {
  submitResult = { accepted: true };
  const { accepted } = mount();
  await act(async () => {});
  await act(async () => {});

  await act(async () => {
    fireEvent.change(screen.getByTestId("wiki-task-channel"), {
      target: { value: CHANNEL_ID },
    });
  });
  await act(async () => {});
  await act(async () => {
    fireEvent.change(screen.getByTestId("wiki-task-agent"), {
      target: { value: AGENT },
    });
    // Uncheck one citation — it must not reach the native input.
    fireEvent.click(screen.getByTestId("wiki-task-ref-src/events.rs"));
    fireEvent.click(screen.getByTestId("wiki-task-start"));
  });
  await act(async () => {});
  await act(async () => {});

  const sequence = calls
    .filter(
      (c) =>
        c.command !== "get_channels" && c.command !== "get_channel_members",
    )
    .map((c) => c.command);
  assert.deepEqual(sequence.slice(0, 3), [
    "owner_operation_scope",
    "wiki_task_dispatch_prepare",
    "wiki_task_dispatch_submit",
  ]);
  const prepare = calls.find((c) => c.command === "wiki_task_dispatch_prepare");
  assert.equal(prepare.payload.expected, SCOPE, "the captured scope fence");
  const input = prepare.payload.input;
  assert.equal(input.draftKey, KEY);
  assert.equal(input.questionId, QUESTION_ID);
  assert.equal(input.attemptId, ATTEMPT_ID);
  assert.equal(input.channelId, CHANNEL_ID);
  assert.equal(input.agentPubkey, AGENT);
  assert.deepEqual(input.references, [
    { path: "src/relay.rs", startLine: 10, endLine: 20 },
  ]);
  assert.equal(
    calls.filter((c) => c.command === "wiki_task_dispatch_submit").length,
    1,
  );

  assert.deepEqual(accepted, [{ channelId: CHANNEL_ID, rootId: EVENT_ID }]);
  // An accepted dispatch retires its draft — the journal now owns the record.
  assert.equal(loadWikiTaskDraft(KEY), null);
  assert.ok(loadDraftEntry(KEY) == null);
});

test("a publish failure keeps the draft and offers reconcile, not a new kickoff", async () => {
  submitResult = { accepted: false };
  mount();
  await act(async () => {});
  await act(async () => {});
  await act(async () => {
    fireEvent.change(screen.getByTestId("wiki-task-channel"), {
      target: { value: CHANNEL_ID },
    });
  });
  await act(async () => {});
  await act(async () => {
    fireEvent.change(screen.getByTestId("wiki-task-agent"), {
      target: { value: AGENT },
    });
    fireEvent.click(screen.getByTestId("wiki-task-start"));
  });
  await act(async () => {});
  await act(async () => {});

  assert.ok(screen.getByTestId("wiki-task-note"));
  // The dispatch id was persisted into the draft before the publish attempt —
  // a retry/resume reuses the same operation and event identity.
  const stored = loadWikiTaskDraft(KEY);
  assert.ok(stored, "the draft survives a failed publish");
  assert.ok(stored.meta.dispatchId, "dispatch id persisted before publish");
  assert.ok(screen.getByTestId("wiki-task-reconcile"));

  // Reconcile settles to accepted — same dispatch id, no second prepare.
  submitResult = {
    accepted: true,
    status: "complete",
    eventId: EVENT_ID,
    rootEventId: EVENT_ID,
    channelId: CHANNEL_ID,
    dispatchId: stored.meta.dispatchId,
    submitAttempts: 1,
    error: null,
  };
  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-task-reconcile"));
  });
  const reconcile = calls.filter(
    (c) => c.command === "wiki_task_dispatch_reconcile",
  );
  assert.equal(reconcile.length, 1);
  assert.equal(reconcile[0].payload.dispatchId, stored.meta.dispatchId);
});
