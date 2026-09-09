import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test, { after, beforeEach } from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

const stubs = new Map([
  [
    "@/features/agent-memory/hooks",
    "export const useIsManagedAgent = () => globalThis.__PRM_MANAGED__ === true;",
  ],
  [
    "@/features/channels/hooks",
    "export const useChannelsQuery = () => ({data: []});",
  ],
  [
    "@/features/projects/hooks",
    "export const projectsQueryKey = ['projects'];",
  ],
  [
    "@/features/profile/hooks",
    "export const useUsersBatchQuery = () => ({data: {profiles: {}}});",
  ],
  [
    "@/features/profile/lib/identity",
    "export const ownsAuthorAgent = () => globalThis.__PRM_AGENT_OWNER__ === true;",
  ],
  [
    "@/features/projects/useAddProjectRepository",
    "export const useAddProjectRepositoryMutation = () => ({isPending: false, mutateAsync: async () => {}});",
  ],
  [
    "@/features/projects/useAttachProjectRepository",
    "export const useAttachProjectRepositoryMutation = () => ({isPending: false, mutateAsync: async () => {}});",
  ],
  [
    "@/features/projects/useBindProjectRepositoryChannel",
    "export const useBindProjectRepositoryChannelMutation = () => ({isPending: false, mutateAsync: async () => {}});",
  ],
  [
    "@/shared/api/projectChannelLink",
    "export const loadProjectChannelLink = async () => ({operation: null}); export const retryProjectRepositoryAttachment = async () => ({value: {status: 'complete', reconciled: true}});",
  ],
  [
    "@tanstack/react-query",
    `
      export const useQuery = (options) => { globalThis.__PRM_QUERY__ = options; return {data: null, refetch: async () => {}}; };
      export const useQueryClient = () => ({invalidateQueries: async () => {}});
    `,
  ],
  [
    "@/shared/ui/button",
    "const React = globalThis.__PRM_REACT__; export const Button = ({children, ...props}) => React.createElement('button', props, children);",
  ],
  [
    "@/shared/ui/dropdown-menu",
    "const React = globalThis.__PRM_REACT__; const Box = ({children}) => React.createElement('div', null, children); export const DropdownMenu = Box, DropdownMenuContent = Box, DropdownMenuItem = Box, DropdownMenuLabel = Box, DropdownMenuTrigger = Box;",
  ],
  [
    "./AddProjectRepositoryDialog",
    "export const AddProjectRepositoryDialog = () => null;",
  ],
  [
    "./AttachProjectRepositoryDialog",
    "export const AttachProjectRepositoryDialog = () => null;",
  ],
  [
    "lucide-react",
    "export const Check = () => null; export const FolderPlus = () => null; export const Link = () => null; export const Plus = () => null; export const ShieldCheck = () => null;",
  ],
  ["sonner", "export const toast = {success() {}, error() {}};"],
]);

globalThis.__PRM_REACT__ = React;
registerHooks({
  resolve(specifier, context, next) {
    return stubs.has(specifier)
      ? {
          shortCircuit: true,
          url: `project-repository-management:${specifier}`,
        }
      : next(specifier, context);
  },
  load(url, context, next) {
    return url.startsWith("project-repository-management:")
      ? {
          shortCircuit: true,
          format: "module",
          source: stubs.get(url.slice("project-repository-management:".length)),
        }
      : next(url, context);
  },
});

const { ProjectRepositoryManagement } = await import(
  "./ProjectRepositoryManagement.tsx"
);

after(() => {
  delete globalThis.__PRM_REACT__;
  delete globalThis.__PRM_MANAGED__;
  delete globalThis.__PRM_AGENT_OWNER__;
  delete globalThis.__PRM_QUERY__;
});

const project = {
  dtag: "project",
  id: "30621:owner:project",
  legacy: false,
  name: "Project",
  owner: "owner",
  projectAddress: "30621:owner:project",
  projectChannelId: null,
  repositories: [],
  repositoryAddresses: [],
};

function render(overrides = {}) {
  globalThis.__PRM_QUERY__ = null;
  renderToStaticMarkup(
    React.createElement(ProjectRepositoryManagement, {
      compact: true,
      identityPubkey: "viewer",
      onChange: () => {},
      project,
      projects: [],
      ...overrides,
    }),
  );
  return globalThis.__PRM_QUERY__;
}

beforeEach(() => {
  globalThis.__PRM_MANAGED__ = false;
  globalThis.__PRM_AGENT_OWNER__ = false;
  delete globalThis.__PRM_QUERY__;
});

test("managed-project owner path keeps native attachment recovery enabled", () => {
  globalThis.__PRM_MANAGED__ = true;
  assert.equal(render().enabled, true);
});

test("agent-controlled path leaves native attachment recovery disabled", () => {
  globalThis.__PRM_AGENT_OWNER__ = true;
  assert.equal(render().enabled, false);
});
