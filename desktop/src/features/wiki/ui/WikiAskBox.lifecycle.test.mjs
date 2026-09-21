/**
 * The developer composer's lifecycle (#366): attempt fences, stop/retry,
 * follow-ups, honest insufficiency, and the scoped history list — driven
 * through the real component against a stubbed Tauri boundary.
 */
import assert from "node:assert/strict";
import { after, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

class NoopObserver {
  disconnect() {}
  observe() {}
  unobserve() {}
}

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  localStorage: dom.window.localStorage,
  window: dom.window,
  ResizeObserver: NoopObserver,
  IntersectionObserver: NoopObserver,
});
if (!dom.window.crypto?.randomUUID) {
  dom.window.crypto = globalThis.crypto;
}

const OWNER = "a".repeat(64);
const AGENT = "b".repeat(64);
const COORDINATE = `${OWNER}:crew`;

let runCalls = [];
let cancelCalls = [];
let forgetCalls = [];
let runResults = [];
let historyItems = [];
let agentList = [];
let runError = null;
const eventHandlers = {};

const ANSWERED = {
  status: "answered",
  markdown: "the grounded answer",
  refusal: null,
  citations: [{ path: "src/lib.rs", startLine: 1, endLine: 3 }],
  sourceRevision: "git:rev",
  manifest: {
    includedPages: [{ slug: "lib", title: "Lib", score: 4 }],
    omittedPages: [],
    includedSources: [{ path: "src/lib.rs", startLine: 1, endLine: 3 }],
    omittedSources: [],
    sourceGrant: true,
  },
  historyRecorded: true,
};

dom.window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: () => {},
};

dom.window.__TAURI_INTERNALS__ = {
  transformCallback: (callback) => callback,
  invoke: async (command, payload) => {
    switch (command) {
      case "plugin:event|listen":
        eventHandlers[payload.event] = payload.handler;
        return 1;
      case "plugin:event|unlisten":
        return null;
      case "private_ask_dev_status":
        return { enabled: true, blockedReason: null };
      case "private_ask_history":
        assert.equal(payload.coordinate, COORDINATE);
        return historyItems;
      case "private_ask_agents":
        return agentList;
      case "private_ask_run": {
        runCalls.push(payload);
        if (runError) throw new Error(runError);
        const result = runResults.shift() ?? { ...ANSWERED };
        return {
          ...result,
          attemptId: payload.attemptId,
          // The backend resolves the thread id itself — a follow-up inherits
          // its parent's — so the stub echoes what the caller sent.
          questionId: payload.questionId,
        };
      }
      case "private_ask_cancel":
        cancelCalls.push(payload);
        return null;
      case "private_ask_forget":
        forgetCalls.push(payload);
        return true;
      case "start_managed_agent_runtime":
        return { status: "starting" };
      default:
        throw new Error(`unexpected command ${command}`);
    }
  },
};

const { act, cleanup, fireEvent, render, screen } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { WikiAskBox } = await import("@/features/wiki/ui/WikiAskBox");

beforeEach(() => {
  cleanup();
  runCalls = [];
  cancelCalls = [];
  forgetCalls = [];
  runResults = [];
  runError = null;
  historyItems = [];
  agentList = [
    { pubkey: AGENT, name: "Scout", relayUrl: "wss://r/", status: "ready" },
  ];
  dom.window.localStorage.clear();
});

after(() => {
  cleanup();
  dom.window.close();
});

async function mount() {
  render(
    React.createElement(WikiAskBox, {
      channelId: "channel-1",
      door: "project",
      owner: OWNER,
      repoD: "crew",
      scopeLabel: "Asking about Crew",
    }),
  );
  await act(async () => {});
  await act(async () => {});
}

async function ask(question = "What does answer do?") {
  fireEvent.change(screen.getByTestId("wiki-ask-dev-input"), {
    target: { value: question },
  });
  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-ask-dev-submit"));
  });
}

test("Enter submits, Shift+Enter and IME composition do not", async () => {
  await mount();
  const input = screen.getByTestId("wiki-ask-dev-input");

  fireEvent.change(input, { target: { value: "line one" } });
  // Shift+Enter is a newline, not a submission.
  fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
  assert.equal(runCalls.length, 0);

  // An in-flight IME composition is never a submission.
  fireEvent.keyDown(input, { key: "Enter", isComposing: true });
  assert.equal(runCalls.length, 0);

  // Enter alone submits.
  await act(async () => {
    fireEvent.keyDown(input, { key: "Enter" });
  });
  assert.equal(runCalls.length, 1);
});

test("stop cancels the named attempt and retry mints a new one", async () => {
  let resolveRun;
  const pending = new Promise((resolve) => {
    resolveRun = resolve;
  });
  const original = dom.window.__TAURI_INTERNALS__.invoke;
  dom.window.__TAURI_INTERNALS__.invoke = async (command, payload) => {
    if (command === "private_ask_run") {
      runCalls.push(payload);
      await pending;
      return {
        ...ANSWERED,
        attemptId: payload.attemptId,
        questionId: payload.questionId,
      };
    }
    return original(command, payload);
  };
  try {
    await mount();
    await act(async () => {
      await ask();
    });
    const attempt = runCalls[0].attemptId;

    // Stop names the in-flight attempt and stands the composer down.
    await act(async () => {
      fireEvent.click(screen.getByTestId("wiki-ask-dev-stop"));
    });
    assert.deepEqual(cancelCalls, [{ attemptId: attempt }]);
    assert.ok(screen.getByTestId("wiki-ask-dev-cancelled"));

    // Retry mints a NEW attempt on the same question thread — the cancelled
    // run's record is its own, never overwritten.
    await act(async () => {
      resolveRun();
      await pending;
    });
    await act(async () => {
      fireEvent.click(screen.getByTestId("wiki-ask-dev-retry"));
    });
    assert.equal(runCalls.length, 2);
    assert.notEqual(runCalls[1].attemptId, attempt);
    assert.equal(runCalls[1].questionId, runCalls[0].questionId);
  } finally {
    dom.window.__TAURI_INTERNALS__.invoke = original;
  }
});

test("a follow-up names its parent attempt and keeps the question thread", async () => {
  await mount();
  await ask("first");
  const parentAttempt = runCalls[0].attemptId;
  const parentQuestion = runCalls[0].questionId;
  assert.ok(screen.getByTestId("wiki-ask-dev-follow-up"));

  await ask("and why?");
  assert.equal(runCalls.length, 2);
  assert.equal(runCalls[1].followUpOf, parentAttempt);
  assert.equal(runCalls[1].questionId, parentQuestion);

  // Detaching starts a fresh thread — an explicit choice, never a guess.
  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-ask-dev-follow-up-detach"));
  });
  await ask("unrelated");
  assert.equal(runCalls[2].followUpOf, null);
  assert.notEqual(runCalls[2].questionId, parentQuestion);
});

test("insufficiency is honest and shows the coverage boundary", async () => {
  runResults = [
    {
      status: "insufficient",
      markdown: "",
      refusal: "not enough source",
      citations: [],
      sourceRevision: "git:rev",
      manifest: {
        includedPages: [],
        omittedPages: [{ slug: "extra", title: "Extra", score: 1 }],
        includedSources: [],
        omittedSources: [{ path: "src/x.rs", startLine: 1, endLine: 9 }],
        sourceGrant: true,
      },
      historyRecorded: true,
    },
  ];
  await mount();
  await ask("something unanswerable");

  assert.ok(screen.getByTestId("wiki-ask-dev-insufficient"));
  assert.ok(screen.getByTestId("wiki-ask-dev-omitted-pages"));
  assert.ok(screen.getByTestId("wiki-ask-dev-omitted-sources"));
  // No invented answer text is shown.
  assert.equal(screen.queryByTestId("wiki-ask-dev-answer"), null);
});

test("history rows open their stored outcome and forget removes the record", async () => {
  const stored = {
    attemptId: "stored-1",
    questionId: "q-9",
    followUpOf: null,
    status: "answered",
    detail: null,
    question: "stored question",
    markdown: "stored answer",
    citations: [{ path: "src/old.rs", startLine: 5, endLine: 8 }],
    manifest: null,
    sourceRevision: "git:stored",
    agentPubkey: AGENT,
    askedAt: 1000,
    finishedAt: 1001,
  };
  historyItems = [stored];

  await mount();
  const row = screen.getByTestId("wiki-ask-dev-history-entry-stored-1");
  await act(async () => {
    fireEvent.click(row);
  });
  assert.equal(
    screen
      .getByTestId("wiki-ask-dev-answer")
      .textContent.includes("stored answer"),
    true,
  );
  assert.equal(
    screen.getByTestId("wiki-ask-dev-source-revision").textContent,
    "source git:stored",
  );

  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-ask-dev-forget-stored-1"));
  });
  assert.deepEqual(forgetCalls, [
    { coordinate: COORDINATE, attemptId: "stored-1" },
  ]);
});

test("a running record on reopen re-attaches so Stop still names it", async () => {
  historyItems = [
    {
      attemptId: "live-1",
      questionId: "q-1",
      followUpOf: null,
      status: "running",
      detail: null,
      question: "still running",
      markdown: null,
      citations: [],
      manifest: null,
      sourceRevision: null,
      agentPubkey: AGENT,
      askedAt: 1000,
      finishedAt: null,
    },
  ];
  await mount();
  // The in-flight run is still owned; the composer re-attaches its id so Stop
  // reaches the real attempt rather than leaving an orphan.
  assert.ok(screen.getByTestId("wiki-ask-dev-stop"));
  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-ask-dev-stop"));
  });
  assert.deepEqual(cancelCalls, [{ attemptId: "live-1" }]);
});

test("an offline agent is shown with a recovery choice, not silently replaced", async () => {
  agentList = [
    { pubkey: AGENT, name: "Scout", relayUrl: "wss://r/", status: "offline" },
  ];
  await mount();
  const offline = screen.getByTestId("wiki-ask-dev-offline");
  assert.ok(offline.textContent.includes("Scout"));
  await act(async () => {
    fireEvent.click(screen.getByTestId(`wiki-ask-dev-start-${AGENT}`));
  });
});
