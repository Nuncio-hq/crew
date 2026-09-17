import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { before, after, afterEach, test } from "node:test";
import * as React from "react";
import { JSDOM } from "jsdom";
import ts from "typescript";

const dom = new JSDOM("<!doctype html><body></body>", {
  url: "http://localhost",
});
before(() =>
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  }),
);
afterEach(async () => (await import("@testing-library/react")).cleanup());
after(() => dom.window.close());

const channelId = "channel";
const threadRootId = "root";
const agentPubkey = "b".repeat(64);

/**
 * A cold mount into a just-cancelled thread renders StoppedRunTrace
 * (`show === false`), so the agent-name lookup must stay enabled through
 * that window — otherwise it reads the empty managed-agents cache and the
 * trace falls back to the generic "Agent" name instead of the real one.
 */
function harness({ managedAgentsCallArgs, outcome }) {
  const deps = {
    react: React,
    "@/features/agents/activeAgentTurnsStore": {
      useActiveAgentsForConversation: () => [],
    },
    "@/features/agents/hooks": {
      useManagedAgentsQuery: (options) => {
        managedAgentsCallArgs.push(options);
        return { data: [{ pubkey: agentPubkey, name: "Ada" }] };
      },
    },
    "@/features/channels/hooks/useChannelUserInput": {
      useChannelUserInput: () => ({ pending: [] }),
    },
    "@/shared/lib/pubkey": {
      normalizePubkey: (pubkey) => pubkey.toLowerCase(),
    },
    "@/features/agents/activeConversationAgentTurnSummaries": {
      useActiveTurnSummariesForConversation: () => [],
    },
    "@/features/agents/recentConversationOutcomes": {
      useRecentOutcomeForConversation: () =>
        outcome !== undefined
          ? outcome
          : {
              outcome: "cancelled",
              agentPubkey,
              endedAt: Date.now(),
              channelId,
            },
    },
    "@/features/agents/conversationId": {
      deriveAgentConversationIdOrNull: (channel, threadRoot) =>
        `${channel}:${threadRoot}`,
    },
    "../lib/workbenchTranscript": {
      userInputBelongsToThread: () => false,
    },
    "../lib/workbenchThreadIndex": {
      findWorkbenchRow: () => null,
    },
    "./useWorkbenchThreadIndex": {
      useWorkbenchThreadIndex: () => ({ rows: [] }),
    },
    "./workbenchRoutes": {
      channelHrefFromWorkbench: () => "",
    },
  };
  function load(path) {
    const source = ts.transpileModule(fs.readFileSync(path, "utf8"), {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        jsx: ts.JsxEmit.React,
        target: ts.ScriptTarget.ES2022,
      },
    }).outputText;
    const exports = {};
    vm.runInNewContext(source, {
      exports,
      require: (id) => {
        if (id in deps) return deps[id];
        throw Error(id);
      },
      React,
      URL,
      console,
    });
    return exports;
  }
  const real = (id, relative) => {
    deps[id] = load(new URL(relative, import.meta.url));
  };
  real("../lib/liveJobDesk", "../lib/liveJobDesk.ts");
  const { useLiveJobDesk } = load(
    new URL("./useLiveJobDesk.ts", import.meta.url),
  );
  return { useLiveJobDesk };
}

test("the managed-agents lookup stays enabled through a just-cancelled thread's stopped trace", async () => {
  const { render, act } = await import("@testing-library/react");
  const managedAgentsCallArgs = [];
  const { useLiveJobDesk } = harness({ managedAgentsCallArgs });
  let result;
  function Probe() {
    result = useLiveJobDesk({ channelId, threadRootId });
    return null;
  }
  await act(async () => {
    render(React.createElement(Probe));
  });
  assert.equal(result.show, false, "no active turn, pending input, or mission");
  assert.equal(
    managedAgentsCallArgs.at(-1)?.enabled,
    true,
    "the query stays enabled while a cancelled outcome is showing",
  );
  assert.equal(
    result.outcome?.outcome,
    "cancelled",
    "the hook exposes the ledger outcome for the caller to render",
  );
  assert.equal(
    result.nameFor(agentPubkey),
    "Ada",
    "the empty managed-agents cache must not win over the real one",
  );
});

test("the managed-agents lookup stays disabled with no active turn and no cancelled outcome", async () => {
  const { render, act } = await import("@testing-library/react");
  const managedAgentsCallArgs = [];
  const { useLiveJobDesk } = harness({ managedAgentsCallArgs, outcome: null });
  let result;
  function Probe() {
    result = useLiveJobDesk({ channelId, threadRootId });
    return null;
  }
  await act(async () => {
    render(React.createElement(Probe));
  });
  assert.equal(result.show, false);
  assert.equal(managedAgentsCallArgs.at(-1)?.enabled, false);
});
