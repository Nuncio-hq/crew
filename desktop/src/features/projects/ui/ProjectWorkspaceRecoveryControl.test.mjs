import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test, { after, afterEach } from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

globalThis.__PROJECT_WORKSPACE_RECOVERY_REACT__ = React;
const stubs = new Map([
  [
    "@tanstack/react-query",
    `export const useQuery = () => globalThis.__PROJECT_WORKSPACE_RECOVERY__.recovery;`,
  ],
  [
    "@/shared/api/projectChannelLink",
    `
      export const loadProjectChannelLink = async (...args) => globalThis.__PROJECT_WORKSPACE_RECOVERY__.calls.push(["load", ...args]);
      export const retryProjectWorkspaceLink = async (...args) => { globalThis.__PROJECT_WORKSPACE_RECOVERY__.calls.push(["link", ...args]); return globalThis.__PROJECT_WORKSPACE_RECOVERY__.retryResult; };
      export const retryProjectWorkspaceUnlink = async (...args) => { globalThis.__PROJECT_WORKSPACE_RECOVERY__.calls.push(["unlink", ...args]); return globalThis.__PROJECT_WORKSPACE_RECOVERY__.retryResult; };
    `,
  ],
  [
    "@/shared/ui/button",
    `const React = globalThis.__PROJECT_WORKSPACE_RECOVERY_REACT__; export function Button(props) { globalThis.__PROJECT_WORKSPACE_RECOVERY__.button = props; return React.createElement("button", {type: props.type}, props.children); }`,
  ],
  [
    "sonner",
    `export const toast = { success: (message) => globalThis.__PROJECT_WORKSPACE_RECOVERY__.toasts.push(["success", message]), error: (message) => globalThis.__PROJECT_WORKSPACE_RECOVERY__.toasts.push(["error", message]) };`,
  ],
]);

registerHooks({
  resolve(specifier, context, next) {
    return stubs.has(specifier)
      ? { shortCircuit: true, url: `project-workspace-recovery:${specifier}` }
      : next(specifier, context);
  },
  load(url, context, next) {
    return url.startsWith("project-workspace-recovery:")
      ? {
          shortCircuit: true,
          format: "module",
          source: stubs.get(url.slice("project-workspace-recovery:".length)),
        }
      : next(url, context);
  },
});

const { ProjectWorkspaceRecoveryControl } = await import(
  "./ProjectWorkspaceRecoveryControl.tsx"
);

after(() => {
  delete globalThis.__PROJECT_WORKSPACE_RECOVERY_REACT__;
});
afterEach(() => {
  delete globalThis.__PROJECT_WORKSPACE_RECOVERY__;
});

const repositoryCoordinate = `30617:${"a".repeat(64)}:crew`;
const operation = {
  id: "operation-1",
  revision: 4,
  payload: {
    action: {
      type: "link-workspace",
      repository_coordinate: repositoryCoordinate,
      channel_id: "12345678-1234-4234-8234-123456789abc",
      local_path: "/Users/oscar/Crew",
    },
  },
};

function render(retryResult) {
  const state = {
    calls: [],
    toasts: [],
    retryResult,
    recovered: 0,
    refetched: 0,
    button: null,
    recovery: {
      data: {
        token: { scope: "owner-scope" },
        operation,
      },
      isFetching: false,
      refetch: async () => {
        state.refetched += 1;
      },
    },
  };
  globalThis.__PROJECT_WORKSPACE_RECOVERY__ = state;
  renderToStaticMarkup(
    React.createElement(ProjectWorkspaceRecoveryControl, {
      onRecovered: () => {
        state.recovered += 1;
      },
      repositoryCoordinate,
    }),
  );
  return state;
}

test("mounted recovery control reports a failed durable retry instead of success", async () => {
  const state = render({
    token: { scope: "owner-scope" },
    value: {
      status: "failed",
      reconciled: false,
      payload: { last_error: "conditional capability unavailable" },
    },
  });
  state.button.onClick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(state.calls, [
    [
      "link",
      { scope: "owner-scope" },
      operation,
      repositoryCoordinate,
      "12345678-1234-4234-8234-123456789abc",
      "/Users/oscar/Crew",
    ],
  ]);
  assert.deepEqual(state.toasts, [
    ["error", "conditional capability unavailable"],
  ]);
  assert.equal(state.refetched, 0);
  assert.equal(state.recovered, 0);
});

test("mounted recovery control only announces a terminal reconciled retry", async () => {
  const state = render({
    token: { scope: "owner-scope" },
    value: {
      status: "complete",
      reconciled: true,
      payload: { last_error: null },
    },
  });
  state.button.onClick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(state.toasts, [
    ["success", "Project workspace link recovered."],
  ]);
  assert.equal(state.refetched, 1);
  assert.equal(state.recovered, 1);
});
