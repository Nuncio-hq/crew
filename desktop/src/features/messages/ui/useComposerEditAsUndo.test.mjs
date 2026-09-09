import assert from "node:assert/strict";
import { after, afterEach, test } from "node:test";
import { createElement } from "react";
import { JSDOM } from "jsdom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { KnownAgentPubkeysProvider } from "../../agents/useKnownAgentPubkeys.tsx";
import { useComposerEditAsUndo } from "./useComposerEditAsUndo.ts";
import { extractMentionPubkeys } from "../lib/extractMentionPubkeys.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
Object.assign(globalThis, {
  window: dom.window,
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
});
const { renderHook, cleanup } = await import("@testing-library/react");
afterEach(cleanup);
after(() => dom.window.close());
const agent = "a".repeat(64);
const human = "b".repeat(64);

for (const [label, refs, unresolved, expected] of [
  [
    "resolved agent",
    [{ pubkey: agent, displayName: "Scout", isAgent: true }],
    [],
    "queued",
  ],
  ["unresolved historical agent", [], [agent, human], "queued"],
  [
    "human only",
    [{ pubkey: human, displayName: "Scout", isAgent: false }],
    [],
    null,
  ],
  ["unbound literal", [], [], null],
]) {
  test(`edit undo uses historical identity for ${label}, before draft hydration`, () => {
    const client = new QueryClient({
      defaultOptions: {
        queries: { enabled: false, retry: false, staleTime: Infinity },
      },
    });
    client.setQueryData(
      ["managed-agents"],
      [{ pubkey: agent, status: "stopped" }],
    );
    client.setQueryData(["relay-agents"], []);
    const wrapper = ({ children }) =>
      createElement(
        QueryClientProvider,
        { client },
        createElement(KnownAgentPubkeysProvider, null, children),
      );
    const { result, unmount } = renderHook(
      () =>
        useComposerEditAsUndo({
          editTarget: {
            id: "historical",
            body: "@Scout hello",
            mentionRefs: refs,
            unresolvedMentionPubkeys: unresolved,
          },
          // The real extraction seam rejects a current same-name roster. Opening an
          // old event must never ask this draft resolver to reinterpret its audience.
          extractMentionPubkeys: (text) =>
            extractMentionPubkeys({
              text,
              selectedMentions: new Map(),
              memberCandidates: [agent, human].map((pubkey) => ({
                pubkey,
                displayName: "Scout",
                isMember: true,
              })),
            }),
        }),
      { wrapper },
    );
    assert.equal(result.current.editAsUndoState, expected);
    unmount();
    client.clear();
  });
}
