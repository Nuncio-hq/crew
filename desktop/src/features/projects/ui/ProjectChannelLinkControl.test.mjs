import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test, { after, afterEach } from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
globalThis.__PROJECT_LINK_REACT__ = React;
after(() => {
  delete globalThis.__PROJECT_LINK_REACT__;
});
const stubs = new Map([
  [
    "@/features/projects/hooks",
    "export const projectsQueryKey = ['projects'];",
  ],
  [
    "@/shared/api/hooks",
    "export const useIdentityQuery = () => ({data:{pubkey:'viewer'}});",
  ],
  [
    "@tanstack/react-query",
    `
    export const useQuery = (options) => { globalThis.__PROJECT_LINK_UI__.query = options; return globalThis.__PROJECT_LINK_UI__.recovery; };
    export const useQueryClient = () => ({invalidateQueries: (options) => {globalThis.__PROJECT_LINK_UI__.invalidations.push(options);}});
    export const useMutation = (options) => {globalThis.__PROJECT_LINK_UI__.mutation = options; return {...globalThis.__PROJECT_LINK_UI__.mutationState, mutate: (resume) => globalThis.__PROJECT_LINK_UI__.calls.push(['mutate',resume])};};
  `,
  ],
  [
    "@/shared/api/projectChannelLink",
    `
    export class ProjectLinkScopeChanged extends Error {}
    export const assertProjectLinkScope = async () => { if(globalThis.__PROJECT_LINK_UI__.stale) throw new ProjectLinkScopeChanged(); };
    export const linkExistingProjectChannel = async (...args) => globalThis.__PROJECT_LINK_UI__.calls.push(['prepare', ...args]);
    export const loadProjectChannelLink = async (...args) => globalThis.__PROJECT_LINK_UI__.calls.push(['load', ...args]);
    export const retryProjectChannelLink = async (...args) => globalThis.__PROJECT_LINK_UI__.calls.push(['retry', ...args]);
  `,
  ],
  [
    "@/shared/ui/button",
    `const React = globalThis.__PROJECT_LINK_REACT__; export function Button(props) {globalThis.__PROJECT_LINK_UI__.buttons.set(props.children,props); return React.createElement('button', {disabled:props.disabled,type:props.type}, props.children);}`,
  ],
  [
    "@/shared/ui/dialog",
    `const React = globalThis.__PROJECT_LINK_REACT__; const Box = ({children}) => React.createElement('div',null,children); export const Dialog=Box,DialogContent=Box,DialogHeader=Box,DialogTitle=Box,DialogTrigger=Box;`,
  ],
]);
registerHooks({
  resolve(specifier, context, next) {
    return stubs.has(specifier)
      ? { shortCircuit: true, url: `project-link-ui:${specifier}` }
      : next(specifier, context);
  },
  load(url, context, next) {
    return url.startsWith("project-link-ui:")
      ? {
          shortCircuit: true,
          format: "module",
          source: stubs.get(url.slice(16)),
        }
      : next(url, context);
  },
});
const { ProjectChannelLinkControl } = await import(
  "./ProjectChannelLinkControl.tsx"
);
const { ProjectLinkScopeChanged } = await import(
  "@/shared/api/projectChannelLink"
);
afterEach(() => {
  delete globalThis.__PROJECT_LINK_UI__;
});
const project = {
  id: "30621:owner:project",
  name: "Project",
  repositories: [],
  projectChannelId: "home",
  relatedChannelIds: ["related"],
};
function render(overrides = {}) {
  const state = {
    calls: [],
    invalidations: [],
    buttons: new Map(),
    mutationState: {},
    recovery: {
      data: { token: { scope: "scope" }, operation: null },
      refetch: async () => {},
    },
    ...overrides,
  };
  globalThis.__PROJECT_LINK_UI__ = state;
  const channels = ["home", "related", "eligible", "archived", "dm"].map(
    (id) => ({
      id,
      name: id,
      isMember: true,
      channelType: id === "dm" ? "dm" : "stream",
      archivedAt: id === "archived" ? "now" : null,
    }),
  );
  const html = renderToStaticMarkup(
    React.createElement(ProjectChannelLinkControl, { project, channels }),
  );
  return { state, html };
}
test("link control lists only unlinked active stream channels", async () => {
  const { state, html } = render();
  assert.match(html, /value="eligible"/);
  for (const id of ["home", "related", "archived", "dm"])
    assert.ok(!html.includes(`value="${id}"`));
  await state.query.queryFn();
  assert.deepEqual(state.calls, [["load", project.id]]);
});
test("pending recovery button retries the exact loaded record", async () => {
  const operation = {
    id: "pending",
    revision: 9,
    payload: { channel_id: "saved-channel", last_error: "ACK unknown" },
  };
  const token = { scope: "captured" };
  const { state, html } = render({
    recovery: { data: { token, operation }, refetch: async () => {} },
  });
  assert.match(html, /saved-channel/);
  state.buttons.get("Retry this operation").onClick();
  await state.mutation.mutationFn(true);
  assert.deepEqual(state.calls, [
    ["mutate", true],
    ["retry", token, operation],
  ]);
});
test("stale mutation completion never invalidates current Project cache", async () => {
  const { state } = render({ stale: true });
  await assert.rejects(
    state.mutation.onSuccess({
      token: {},
      value: { status: "complete", reconciled: true },
    }),
    ProjectLinkScopeChanged,
  );
  assert.deepEqual(state.invalidations, []);
});
test("resolved conflict is labeled for a reviewed successor, not a successful link", () => {
  const { html } = render({
    mutationState: {
      data: { value: { status: "superseded", reconciled: true } },
    },
  });
  assert.match(html, /Review its refreshed details/);
  assert.doesNotMatch(html, /successfully linked/);
});
