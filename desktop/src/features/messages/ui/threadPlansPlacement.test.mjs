import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import test from "node:test";
import * as React from "react";
import * as jsx from "react/jsx-runtime";
import { renderToStaticMarkup } from "react-dom/server";
import ts from "typescript";
import { DeclaredPlansRail } from "./DeclaredPlansRail.tsx";

test("thread body no longer mounts an extra declared-plan rail", () => {
  const exports = {};
  const noop = () => null;
  const dependencies = {
    react: React,
    "react/jsx-runtime": jsx,
    "@/features/messages/ui/ProjectThreadWorkspacePanel": {
      ProjectThreadWorkspacePanel: noop,
    },
    "@/features/agents/ui/useObserverEvents": {
      useLoadArchivedObserverEvents: () => ({
        fetchOlderArchived: async () => {},
        hasOlderArchived: false,
      }),
    },
    "@/features/messages/lib/threadForgeViewContextStore": {
      setThreadForgeViewContext: noop,
    },
    "@/shared/hooks/use-mobile": {
      useElementWidth: () => [null, 900],
      useIsThreadPanelOverlay: () => false,
    },
    "@/shared/lib/cn": { cn: (...values) => values.filter(Boolean).join(" ") },
    "@/shared/layout/AuxiliaryPanel": { getAuxiliaryPanelBodyClass: () => "" },
    "@/shared/layout/responsiveContract": {
      shouldStackDeclaredPlansRail: () => false,
    },
    "./DeclaredPlansRail": { DeclaredPlansRail },
    "./useDeclaredPlansForThread": {
      useDeclaredPlansForThread: () => ({
        plans: [
          {
            agentPubkey: "a".repeat(64),
            agentName: "Dev",
            unknown: true,
            liveness: "idle",
            entries: [],
            updatedAt: null,
          },
        ],
      }),
    },
    "@/features/tool-pane/toolPaneStore": {
      openThreadToolPane: noop,
      useToolPane: () => ({ open: false, tab: "context" }),
    },
    "@/features/channels/lib/threadViewModePreference": {
      setThreadViewMode: noop,
    },
  };
  vm.runInNewContext(
    ts.transpileModule(
      fs.readFileSync(
        new URL("./ThreadPanelDeclaredPlansBody.tsx", import.meta.url),
        "utf8",
      ),
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
  const html = renderToStaticMarkup(
    React.createElement(
      exports.ThreadPanelDeclaredPlansBody,
      {
        channelId: "channel",
        threadHead: { id: "a".repeat(64) },
        threadMessages: [],
        isFocusMode: true,
        isHuddleTranscript: false,
        panelChromeMode: "split",
        workspaceModel: null,
      },
      React.createElement("p", null, "Conversation remains here"),
    ),
  );
  assert.doesNotMatch(html, /data-testid="declared-plans-rail"/);
  assert.match(html, /Conversation remains here/);
});
