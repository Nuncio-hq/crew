import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";

const stubs = new Map([
  [
    "@/features/projects/ui/CrewProjectOverviewScreen",
    "export const CrewProjectOverviewScreen = 'overview';",
  ],
  [
    "@/features/projects/hooks",
    "export const useProjectQuery = () => globalThis.__PROJECT_GATE_QUERY__;",
  ],
  [
    "@/features/projects/ui/CrewCoworkProjectScreen",
    "export const CrewCoworkProjectScreen = 'cowork';",
  ],
  [
    "@/features/projects/ui/ProjectDetailScreen",
    "export const ProjectDetailScreen = 'detail';",
  ],
]);
registerHooks({
  resolve(specifier, context, nextResolve) {
    return stubs.has(specifier)
      ? { shortCircuit: true, url: `gate-stub:${specifier}` }
      : nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    return url.startsWith("gate-stub:")
      ? {
          shortCircuit: true,
          format: "module",
          source: stubs.get(url.slice(10)),
        }
      : nextLoad(url, context);
  },
});
const { CrewProjectDetailGate } = await import("./CrewProjectDetailGate.tsx");
const owner = "a".repeat(64);
const git = {
  id: `${owner}:git`,
  repoAddress: `30617:${owner}:git`,
  workspaceMode: "git",
};
const folder = {
  id: `${owner}:folder`,
  repoAddress: `30617:${owner}:folder`,
  workspaceMode: "folder",
};
const projectId = `30621:${owner}:mixed`;
function route(props = {}) {
  globalThis.__PROJECT_GATE_QUERY__ = {
    isPending: false,
    data: {
      repositories: [git, folder],
      primaryRepositoryAddress: git.repoAddress,
    },
  };
  return CrewProjectDetailGate({ projectId, ...props });
}

test("mixed Project with Git primary never opens an unrelated folder workspace", () => {
  assert.equal(route().type, "overview");
  assert.equal(route({ repositoryId: git.repoAddress }).type, "overview");
});
test("explicit folder selection opens overview with the same repository coordinate", () => {
  const result = route({ repositoryId: folder.repoAddress });
  assert.equal(result.type, "overview");
  assert.equal(result.props.repositoryId, folder.repoAddress);
});
test("missing explicit repository cannot fall back to a folder workspace", () => {
  const missing = `30617:${owner}:removed`;
  const result = route({ repositoryId: missing });
  assert.equal(result.type, "overview");
  assert.equal(result.props.repositoryId, missing);
});
test("explicit Wiki navigation survives a folder member", () => {
  const result = route({ repositoryId: folder.repoAddress, tab: "wiki" });
  assert.equal(result.type, "detail");
  assert.equal(result.props.tab, "wiki");
  assert.equal(result.props.repositoryId, folder.repoAddress);
});

test("thread history deep links keep their exact folder destination", () => {
  const result = route({
    repositoryId: folder.repoAddress,
    thread: "thread-a",
  });
  assert.equal(result.type, "cowork");
  assert.equal(result.props.threadId, "thread-a");
});
test("file deep links retain repository detail", () => {
  const result = route({
    repositoryId: git.repoAddress,
    filePath: "README.md",
  });
  assert.equal(result.type, "detail");
  assert.equal(result.props.filePath, "README.md");
});

// Explicit tabs select the existing repository detail surface, not the Project landing page.
test("workspace overview tab opens exact repository details", () => {
  const result = route({ repositoryId: folder.repoAddress, tab: "overview" });
  assert.equal(result.type, "detail");
  assert.equal(result.props.tab, "overview");
  assert.equal(result.props.repositoryId, folder.repoAddress);
});
test("Git thread routes retain the existing repository detail branch", () => {
  const result = route({ repositoryId: git.repoAddress, thread: "thread-a" });
  assert.equal(result.type, "detail");
  assert.equal(result.props.repositoryId, git.repoAddress);
});
