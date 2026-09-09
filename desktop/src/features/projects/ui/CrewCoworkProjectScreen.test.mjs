import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

const stubs = new Map([
  [
    "@/features/projects/hooks",
    "export const useProjectQuery = () => globalThis.__COWORK_PROJECT_QUERY__;",
  ],
  [
    "@/app/navigation/useAppNavigation",
    "export const useAppNavigation = () => ({});",
  ],
  [
    "@tanstack/react-query",
    "export const useQuery = (input) => { globalThis.__COWORK_HISTORY_QUERY__ = input; return {}; }; export const useMutation = () => ({}); export const useQueryClient = () => ({});",
  ],
  [
    "@/shared/api/coworkVersions",
    "export const listCoworkVersions = async (input) => input; export const restoreCoworkFile = () => {}; export const restoreCoworkFolder = () => {}; export const compactCoworkHistory = () => {};",
  ],
  ["@/shared/ui/button", "export const Button = 'button';"],
]);
registerHooks({
  resolve(specifier, context, nextResolve) {
    return stubs.has(specifier)
      ? { shortCircuit: true, url: `cowork-stub:${specifier}` }
      : nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    return url.startsWith("cowork-stub:")
      ? {
          shortCircuit: true,
          format: "module",
          source: stubs.get(url.slice(12)),
        }
      : nextLoad(url, context);
  },
});
const { CrewCoworkProjectScreen } = await import(
  "./CrewCoworkProjectScreen.tsx"
);
const owner = "a".repeat(64);
const git = {
  id: `${owner}:git`,
  repoAddress: `30617:${owner}:git`,
  workspaceMode: "git",
  localWorkspacePath: "/work/git",
};
const folder = {
  id: `${owner}:folder`,
  repoAddress: `30617:${owner}:folder`,
  workspaceMode: "folder",
  localWorkspacePath: "/work/folder",
};
function render(repositoryId) {
  globalThis.__COWORK_PROJECT_QUERY__ = {
    data: {
      name: "Mixed",
      repositories: [git, folder],
      primaryRepositoryAddress: git.repoAddress,
    },
  };
  return renderToStaticMarkup(
    React.createElement(CrewCoworkProjectScreen, {
      projectId: `30621:${owner}:mixed`,
      repositoryId,
    }),
  );
}
test("selected nonprimary folder binds the real history query to its exact path and coordinate", async () => {
  assert.match(render(folder.repoAddress), /\/work\/folder/);
  const query = globalThis.__COWORK_HISTORY_QUERY__;
  assert.equal(query.enabled, true);
  assert.deepEqual(await query.queryFn(), {
    folder: "/work/folder",
    repoAddress: folder.repoAddress,
  });
});
test("missing or Git selections never enable folder history operations", () => {
  for (const selection of [git.repoAddress, `30617:${owner}:removed`]) {
    assert.match(render(selection), /could not be found/);
    assert.equal(globalThis.__COWORK_HISTORY_QUERY__.enabled, false);
  }
});
