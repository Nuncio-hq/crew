import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { after, afterEach, before, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";
import * as React from "react";
import * as jsx from "react/jsx-runtime";
import ts from "typescript";
import * as forgeContext from "@/features/messages/lib/threadForgeViewContextStore.ts";
import * as toolPaneStore from "@/features/tool-pane/toolPaneStore.ts";

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
beforeEach(() => {
  forgeContext.resetThreadForgeViewContext();
  toolPaneStore.resetToolPaneForTests();
});
afterEach(async () => (await import("@testing-library/react")).cleanup());
after(() => dom.window.close());

const AGENT_A = "a".repeat(64);
const AGENT_B = "b".repeat(64);

function load(sourcePath, dependencies) {
  const exports = {};
  vm.runInNewContext(
    ts.transpileModule(
      fs.readFileSync(new URL(sourcePath, import.meta.url), "utf8"),
      {
        compilerOptions: {
          module: ts.ModuleKind.CommonJS,
          target: ts.ScriptTarget.ES2022,
          jsx: ts.JsxEmit.ReactJSX,
        },
      },
    ).outputText,
    {
      exports,
      require: (key) => {
        assert.ok(key in dependencies, `unmocked dependency ${key}`);
        return dependencies[key];
      },
    },
  );
  return exports;
}

function makeLoader(calls) {
  return function useLoadArchivedObserverEvents(enabled, channelId) {
    const owner = React.useRef(null);
    if (!owner.current) {
      const id = ++calls.instances;
      owner.current = {
        id,
        paging: {
          fetchOlderArchived: async () => calls.fetches.push(id),
          hasOlderArchived: true,
        },
      };
      calls.paging.push(owner.current.paging);
    }
    React.useEffect(() => {
      calls.enables.push({ channelId, enabled, owner: owner.current.id });
      if (!enabled) return;
      calls.starts.push(owner.current.id);
      return () => calls.stops.push(owner.current.id);
    }, [channelId, enabled]);
    return owner.current.paging;
  };
}

function loadSharedModules(calls) {
  const useLoadArchivedObserverEvents = makeLoader(calls);
  const observerHooks = {
    useArchivedChannelEvents: () => [],
    useLoadArchivedObserverEvents,
    useObserverEvents: () => ({ events: [] }),
  };
  const projection = {
    collectParticipatingAgentPubkeys: ({ knownAgentPubkeys }) =>
      knownAgentPubkeys,
    latestSessionIdFromEvents: () => null,
    projectDeclaredPlansForThread: (_conversationId, agents) =>
      agents.map((agent) => ({
        ...agent,
        entries: [],
        unknown: false,
      })),
    resolveAgentPlanName: (pubkey, _profiles, managedName) =>
      managedName ?? pubkey,
  };
  const planDependencies = {
    react: React,
    "@/features/agents/activeConversationAgentTurnSummaries": {
      useActiveTurnSummariesForConversation: () => [],
    },
    "@/features/agents/conversationId": {
      deriveAgentConversationIdOrNull: (channelId, rootId) =>
        channelId && rootId ? `${channelId}:${rootId}` : null,
    },
    "@/features/agents/declaredPlanProjection": projection,
    "@/features/agents/hooks": {
      useManagedAgentsQuery: () => ({ data: [] }),
    },
    "@/features/agents/managedAgentRuntimeHooks": {
      useManagedAgentRuntimesQuery: () => ({ data: [] }),
    },
    "@/features/agents/managedAgentRuntimeStatus": {
      findManagedAgentRuntime: () => undefined,
      isManagedAgentRuntimeSleeping: () => false,
    },
    "@/features/agents/observerRelayStore": {
      getAgentObserverSnapshot: () => ({
        connectionState: "open",
        events: [],
      }),
      getArchivedChannelEvents: () => [],
      subscribeAgentObserverProjections: () => () => {},
      subscribeAgentObserverStore: () => () => {},
    },
    "@/features/agents/ui/agentSessionPanelLayout": {
      mergeObserverEventWindows: () => [],
    },
    "@/features/agents/ui/useObserverEvents": observerHooks,
    "@/features/agents/useKnownAgentPubkeys": {
      useKnownAgentPubkeys: () => [AGENT_A, AGENT_B],
    },
    "@/features/communities/useCommunities": {
      useCommunities: () => ({
        activeCommunity: { relayUrl: "wss://crew.example" },
      }),
    },
    "@/shared/lib/pubkey": {
      normalizePubkey: (pubkey) => pubkey,
    },
  };
  const plansModule = load("./useDeclaredPlansForThread.ts", planDependencies);

  const icons = new Proxy({}, { get: () => () => null });
  const missionControl = {
    createProjectThreadPeekFeedSelector: () => () => [],
    formatProjectThreadPeekText: (text) => text,
    getProjectThreadPeekHeadline: () => "Working",
    mergeProjectThreadPeekEvents: () => [],
    previewProjectThreadPeekText: (text) => ({
      preview: text,
      truncated: false,
    }),
    resolveProjectThreadPeekMode: () => "live",
  };
  const peekModule = load("./ProjectThreadActivityPeek.tsx", {
    react: React,
    "react/jsx-runtime": jsx,
    "lucide-react": icons,
    "@/features/agents/ui/agentSessionTranscript": {
      buildTranscriptState: () => ({ items: [] }),
    },
    "@/features/agents/ui/useObserverEvents": observerHooks,
    "../lib/projectThreadMissionControl": missionControl,
    "@/shared/lib/cn": { cn: (...values) => values.filter(Boolean).join(" ") },
    "@/features/messages/lib/threadForgeViewContextStore": forgeContext,
    "@/features/tool-pane/toolPaneStore": toolPaneStore,
  });
  const infoModule = load("../../tool-pane/ThreadInformationTab.tsx", {
    react: React,
    "react/jsx-runtime": jsx,
    "@/features/messages/lib/threadForgeViewContextStore": forgeContext,
    "@/features/messages/ui/useDeclaredPlansForThread": plansModule,
    "@/features/messages/ui/DeclaredPlansRail": {
      DeclaredPlansRail: () => null,
    },
    "./ThreadActivityRunControls": {
      ThreadActivityRunControls: () => null,
    },
    "./ThreadAgentTranscript": {
      ThreadAgentTranscript: ({ agent, archivePaging }) => {
        calls.transcriptPaging.push(archivePaging);
        return React.createElement("p", null, agent.agentName);
      },
    },
  });
  const bodyModule = load("./ThreadPanelDeclaredPlansBody.tsx", {
    react: React,
    "react/jsx-runtime": jsx,
    "@/features/agents/ui/useObserverEvents": observerHooks,
    "@/features/messages/ui/ProjectThreadWorkspacePanel": {
      ProjectThreadWorkspacePanel: () => null,
    },
    "@/features/messages/lib/threadForgeViewContextStore": forgeContext,
    "@/shared/lib/cn": { cn: (...values) => values.filter(Boolean).join(" ") },
    "@/shared/layout/AuxiliaryPanel": {
      getAuxiliaryPanelBodyClass: () => "",
    },
    "@/features/tool-pane/toolPaneStore": toolPaneStore,
  });
  return {
    Body: bodyModule.ThreadPanelDeclaredPlansBody,
    Peek: peekModule.ProjectThreadActivityPeek,
    Information: infoModule.ThreadInformationTab,
  };
}

const model = {
  activeName: "Worker A",
  activePubkey: AGENT_A,
  conversationId: "channel:root",
  steps: [{ status: "working" }],
};

test("composed thread keeps one archive owner through Activity transitions", async () => {
  const { render, fireEvent, act } = await import("@testing-library/react");
  const calls = {
    enables: [],
    fetches: [],
    instances: 0,
    paging: [],
    starts: [],
    stops: [],
    transcriptPaging: [],
  };
  const { Body, Peek, Information } = loadSharedModules(calls);
  const threadHead = { id: "root", body: "Task" };

  function ComposedThread() {
    const pane = toolPaneStore.useToolPane();
    return React.createElement(
      React.Fragment,
      null,
      React.createElement(
        Body,
        {
          channelId: "channel",
          isFocusMode: true,
          isHuddleTranscript: false,
          panelChromeMode: "embedded",
          profiles: {},
          threadHead,
          threadMessages: [],
          workspaceModel: model,
        },
        React.createElement(Peek, { channelId: "channel", model }),
      ),
      pane.open && pane.tab === "activity"
        ? React.createElement(Information, {
            channelId: "channel",
            tab: "activity",
            threadRootId: "root",
          })
        : null,
    );
  }

  const view = render(React.createElement(ComposedThread));
  assert.equal(calls.instances, 1);
  assert.deepEqual(calls.starts, [1]);

  await act(async () => toolPaneStore.openToolPane("activity"));
  assert.equal(calls.instances, 1);
  assert.deepEqual(calls.starts, [1]);
  const sharedPaging = calls.transcriptPaging.at(-1);
  assert.equal(
    sharedPaging.fetchOlderArchived,
    calls.paging[0].fetchOlderArchived,
  );

  fireEvent.change(view.getByRole("combobox", { name: "Activity agent" }), {
    target: { value: AGENT_B },
  });
  assert.equal(calls.instances, 1);
  assert.deepEqual(calls.starts, [1]);
  assert.equal(calls.transcriptPaging.at(-1), sharedPaging);

  await act(async () => toolPaneStore.closeToolPane());
  assert.equal(calls.instances, 1);
  assert.deepEqual(calls.starts, [1]);
  assert.deepEqual(calls.stops, []);
  assert.equal(view.queryByRole("combobox", { name: "Activity agent" }), null);

  view.unmount();
  assert.deepEqual(calls.stops, [1]);
});
