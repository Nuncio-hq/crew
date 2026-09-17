import assert from "node:assert/strict";
import { after, afterEach, before, mock, test } from "node:test";

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
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: dom.window.navigator,
  writable: true,
});
dom.window.matchMedia = () => ({
  matches: true,
  addEventListener() {},
  removeEventListener() {},
});

let cleanup;
let act;
let render;
let screen;
let buildOwnerFields;

before(async () => {
  ({ act, cleanup, render, screen } = await import("@testing-library/react"));
  ({ buildOwnerFields } = await import("./UserProfilePanelFields.tsx"));
});

afterEach(() => cleanup());
after(() => dom.window.close());

const PUBKEY = "a".repeat(64);

function managedAgent() {
  return {
    agentCommand: "hermes",
    backend: { type: "local" },
    status: "running",
  };
}

function runtime() {
  return {
    error: null,
    lifecycle: "ready",
    localSetup: true,
    logPath: null,
    pid: 42,
    pubkey: PUBKEY,
    relayUrl: "ws://127.0.0.1:3000",
    transport: {
      attempts: 6,
      code: "timeout",
      elapsedMs: 26_400,
      lastError: null,
      nextRetryAtMs: null,
      state: "exhausted",
    },
    transportRetired: false,
  };
}

test("profile runtime keeps process status separate from exhausted transport", () => {
  const fields = buildOwnerFields({
    includeOperationalFields: true,
    managedAgent: managedAgent(),
    managedAgentRuntime: runtime(),
    ownerDisplayName: "Owner",
    ownerHandle: "owner",
    ownerProfilePubkey: PUBKEY,
    ownerPubkey: PUBKEY,
    persona: undefined,
    presenceLoaded: true,
    presenceStatus: "offline",
    relayAgent: undefined,
  });
  const status = fields.find((field) => field.label === "Status");
  assert.ok(status?.displayNode, "the runtime status row must render");

  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    render(status.displayNode);
    act(() => mock.timers.tick(15_001));

    assert.ok(screen.getByText("Running"));
    assert.ok(screen.getByTestId("user-profile-agent-transport"));
    assert.ok(screen.getByText("Connection retries exhausted"));
    assert.ok(screen.getByText("The connection retry limit was reached."));
    assert.equal(screen.queryByText("Starting…"), null);
  } finally {
    mock.timers.reset();
  }
});

test("the transport detail is announced once, not as badge tooltip and text", () => {
  const fields = buildOwnerFields({
    includeOperationalFields: true,
    managedAgent: managedAgent(),
    managedAgentRuntime: runtime(),
    ownerDisplayName: "Owner",
    ownerHandle: "owner",
    ownerProfilePubkey: PUBKEY,
    ownerPubkey: PUBKEY,
    persona: undefined,
    presenceLoaded: true,
    presenceStatus: "offline",
    relayAgent: undefined,
  });
  const status = fields.find((field) => field.label === "Status");
  assert.ok(status?.displayNode, "the runtime status row must render");

  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    render(status.displayNode);
    act(() => mock.timers.tick(15_001));

    const badge = screen.getByTestId("user-profile-agent-transport");
    const detail = "The connection retry limit was reached.";
    assert.ok(screen.getByText(detail), "the detail renders as visible text");
    assert.equal(
      badge.getAttribute("title"),
      null,
      "a title would repeat the visible detail as a second screen-reader stop",
    );
  } finally {
    mock.timers.reset();
  }
});
