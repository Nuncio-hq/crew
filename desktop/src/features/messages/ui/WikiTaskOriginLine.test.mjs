/**
 * #367 — the source backlink on a dispatched Wiki task kickoff. Author and
 * other-viewer paths resolve through the real component against the stubbed
 * Tauri boundary: the author's op record restores the private attempt; a
 * second viewer can only reach the coordinate's authorized Wiki surface.
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
const COORDINATE = `${OWNER}:crew`;
const DISPATCH_ID = "55555555-5555-4555-8555-555555555555";
const ATTEMPT_ID = "22222222-2222-4222-8222-222222222222";

const PROJECT_ID = `30621:${OWNER}:crew`;

const SCOPE = {
  scope: { owner: OWNER, community: "ws://relay" },
  workspace_generation: 1,
  identity_generation: 1,
};

const TAGS = [
  ["h", "33333333-3333-4333-8333-333333333333"],
  ["client", "crew-wiki-task", DISPATCH_ID],
  ["client", "crew-wiki-task-origin", COORDINATE],
];

let loadedOps = [];
let opResult = null;

dom.window.__TAURI_INTERNALS__ = {
  transformCallback: (callback) => callback,
  invoke: async (command, payload) => {
    switch (command) {
      case "owner_operation_scope":
        return SCOPE;
      case "owner_operation_load":
        loadedOps.push(payload);
        if (opResult === "missing") {
          throw new Error("operation not found");
        }
        return opResult;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  },
};

const { act, cleanup, fireEvent, render, screen } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { WikiTaskOriginLine } = await import(
  "@/features/messages/ui/WikiTaskOriginLine"
);
const { peekWikiAskFocus, readWikiTaskOrigin } = await import(
  "@/features/wiki/lib/wikiTaskOrigin"
);

beforeEach(() => {
  cleanup();
  loadedOps = [];
  opResult = {
    token: SCOPE,
    value: {
      id: DISPATCH_ID,
      kind: "thread-handoff",
      payload: {
        draftKey: `wiki:task:${PROJECT_ID}:${COORDINATE}:${ATTEMPT_ID}`,
      },
    },
  };
});

after(() => {
  cleanup();
  dom.window.close();
});

test("reads only the two client markers and ignores foreign tags", () => {
  assert.equal(readWikiTaskOrigin(undefined), null);
  assert.equal(readWikiTaskOrigin([["client", "crew-wiki-task"]]), null);
  assert.deepEqual(readWikiTaskOrigin(TAGS), {
    dispatchId: DISPATCH_ID,
    coordinate: COORDINATE,
  });
});

test("renders nothing on a message without the origin markers", () => {
  const { container } = render(
    React.createElement(WikiTaskOriginLine, { tags: [] }),
  );
  assert.equal(
    container.querySelector("[data-testid='wiki-task-origin']"),
    null,
  );
});

test("the author path restores the private attempt and project wiki", async () => {
  const navigations = [];
  render(
    React.createElement(WikiTaskOriginLine, {
      navigate: (target) => navigations.push(target),
      tags: TAGS,
    }),
  );
  const chip = screen.getByTestId("wiki-task-origin-link");
  assert.match(chip.textContent, /private Wiki answer.*crew/);

  await act(async () => {
    fireEvent.click(chip);
  });
  await act(async () => {});

  assert.equal(loadedOps.length, 1);
  assert.equal(loadedOps[0].id, DISPATCH_ID);
  // projectId is a NIP-MP coordinate (`30621:<pk>:<d>`) — itself
  // colon-bearing; the parse must anchor on the repository coordinate's
  // 64-hex owner pubkey or the route truncates to `30621`. The route's
  // validateSearch accepts only the `30617:`-prefixed coordinate form.
  assert.deepEqual(navigations, [
    {
      kind: "project",
      projectId: PROJECT_ID,
      repositoryAddress: `30617:${COORDINATE}`,
    },
  ]);
  // The private attempt id was stashed for the composer to restore — it never
  // entered the event itself.
  assert.deepEqual(peekWikiAskFocus(COORDINATE), { attemptId: ATTEMPT_ID });
});

test("another viewer reaches only the coordinate's authorized wiki", async () => {
  opResult = "missing";
  const navigations = [];
  render(
    React.createElement(WikiTaskOriginLine, {
      navigate: (target) => navigations.push(target),
      resolveProjects: async () => [
        { id: "proj-9", repositoryAddresses: [`30617:${COORDINATE}`] },
      ],
      tags: TAGS,
    }),
  );
  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-task-origin-link"));
  });
  await act(async () => {});
  await act(async () => {});

  // The author's private attempt is never touched — the resolved project wiki
  // is all the viewer's own access authorizes. repositoryAddresses carry the
  // `30617:` kind prefix; the lookup must match on that form.
  assert.deepEqual(navigations, [
    {
      kind: "project",
      projectId: "proj-9",
      repositoryAddress: `30617:${COORDINATE}`,
    },
  ]);
});

test("an unresolvable origin still lands on the library surface", async () => {
  opResult = "missing";
  const navigations = [];
  render(
    React.createElement(WikiTaskOriginLine, {
      navigate: (target) => navigations.push(target),
      resolveProjects: async () => [],
      tags: TAGS,
    }),
  );
  await act(async () => {
    fireEvent.click(screen.getByTestId("wiki-task-origin-link"));
  });
  await act(async () => {});
  await act(async () => {});

  assert.deepEqual(navigations, [{ kind: "library" }]);
});
