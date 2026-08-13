import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const viewUrl = new URL("./ui/ProjectsView.tsx", import.meta.url);
const menuUrl = new URL("./ui/ProjectsCreateMenu.tsx", import.meta.url);
const screenUrl = new URL("./ui/crew-projects-screen.tsx", import.meta.url);
const flowUrl = new URL("./ui/crew-add-project-flow.tsx", import.meta.url);
const cardsUrl = new URL("./ui/ProjectCards.tsx", import.meta.url);
const snapshotsUrl = new URL("./useProjectsRepoSnapshots.ts", import.meta.url);
const hooksUrl = new URL("./hooks.ts", import.meta.url);
const syncHooksUrl = new URL("./repoSyncHooks.ts", import.meta.url);
const detailUrl = new URL("./ui/ProjectDetailScreen.tsx", import.meta.url);
const commitDiffUrl = new URL("./useProjectCommitDiff.ts", import.meta.url);
const reviewCardUrl = new URL(
  "./ui/PullRequestReviewCard.tsx",
  import.meta.url,
);
const pullRequestMutationsUrl = new URL(
  "./pullRequestMutations.ts",
  import.meta.url,
);

test("ProjectsView keeps the plus menu when the relay has no Projects", async () => {
  const source = await readFile(viewUrl, "utf8");

  assert.equal(source.includes("if (projects.length === 0)"), false);
  assert.match(source, /projects\.length === 0\s*\?\s*\(\s*<EmptyState/);
  assert.match(source, /onCreateRepository\?\.\(\)/);
});

test("a local-only Project cannot fall through to Clone and open Terminal", async () => {
  const [
    cards,
    snapshots,
    hooks,
    syncHooks,
    detail,
    commitDiff,
    reviewCard,
    pullRequestMutations,
  ] = await Promise.all([
    readFile(cardsUrl, "utf8"),
    readFile(snapshotsUrl, "utf8"),
    readFile(hooksUrl, "utf8"),
    readFile(syncHooksUrl, "utf8"),
    readFile(detailUrl, "utf8"),
    readFile(commitDiffUrl, "utf8"),
    readFile(reviewCardUrl, "utf8"),
    readFile(pullRequestMutationsUrl, "utf8"),
  ]);

  assert.match(cards, /canOpenTerminal/);
  assert.match(cards, /canOpenTerminal\s*\?\s*\(/);
  assert.ok(
    snapshots.indexOf("readProjectLocalRepoSnapshot") <
      snapshots.indexOf("if (repository.localWorkspacePath) return null"),
  );
  const localSnapshotHook = hooks.slice(
    hooks.indexOf("export function useProjectLocalRepoSnapshotQuery"),
    hooks.indexOf("export function useProjectLocalRepositoriesQuery"),
  );
  assert.match(
    localSnapshotHook,
    /project\?\.localWorkspaceStatus !== "invalid"/,
  );
  const remoteSnapshotHook = hooks.slice(
    hooks.indexOf("export function useProjectRepoSnapshotQuery"),
    hooks.indexOf("export function useProjectRepoDiffQuery"),
  );
  assert.match(remoteSnapshotHook, /!project\?\.localWorkspacePath/);
  const remoteDiffHook = hooks.slice(
    hooks.indexOf("export function useProjectRepoDiffQuery"),
    hooks.indexOf("export function useProjectLocalRepoDiffQuery"),
  );
  assert.match(remoteDiffHook, /!project\?\.localWorkspacePath/);
  const localDiffHook = hooks.slice(
    hooks.indexOf("export function useProjectLocalRepoDiffQuery"),
    hooks.indexOf("export function useProjectLocalRepoSnapshotQuery"),
  );
  assert.match(localDiffHook, /localWorkspaceStatus !== "invalid"/);
  assert.match(syncHooks, /!project\?\.localWorkspacePath/);
  assert.match(syncHooks, /project\?\.localWorkspacePath \?\? "managed"/);
  assert.match(syncHooks, /Linked workspaces are read-only/);
  assert.match(detail, /!isLinkedWorkspace &&/);
  assert.match(
    detail,
    /Boolean\(hasLocalCheckout \|\| firstCloneUrl\(repository\)\)/,
  );
  assert.match(detail, /canPush:\s*!isLinkedWorkspace/);
  assert.match(detail, /canPull:\s*!isLinkedWorkspace/);
  assert.match(detail, /onFetch: isLinkedWorkspace\s*\?\s*undefined/);
  assert.match(
    detail,
    /onOpenMergeRecoveryTerminal=\{\s*isLinkedWorkspace\s*\?\s*undefined/,
  );
  assert.match(detail, /repoSource=\{effectiveRepoSource\}/);
  assert.match(commitDiff, /!project\.localWorkspacePath/);
  assert.match(commitDiff, /project\.localWorkspaceStatus !== "invalid"/);
  assert.match(localSnapshotHook, /localWorkspaceStatus !== "invalid"/);
  assert.match(reviewCard, /!project\.localWorkspacePath/);
  assert.match(reviewCard, /project\.localWorkspaceStatus !== "invalid"/);
  assert.match(pullRequestMutations, /Linked workspaces are read-only/);
  assert.match(snapshots, /repository\?\.localWorkspacePath \?\? "managed"/);
});

test("Crew uses one folder-first callback and removes the standalone workspace strip", async () => {
  const [menu, screen, flow] = await Promise.all([
    readFile(menuUrl, "utf8"),
    readFile(screenUrl, "utf8"),
    readFile(flowUrl, "utf8"),
  ]);

  assert.match(menu, /Repository/);
  assert.doesNotMatch(menu, /\n\s+Project\n/);
  assert.match(menu, /30617 = repository/);
  assert.match(menu, /30621 Project surface yet/);
  assert.match(screen, /CrewAddProjectFlow/);
  assert.match(screen, /onCreateRepository=\{chooseFolder\}/);
  assert.equal(screen.includes("CrewProjectWorkspacePanel"), false);
  assert.ok(
    flow.indexOf("const path = await chooseProjectWorkspaceFolder") <
      flow.indexOf("await createCurrentLocalWorkspaceProject"),
  );
  assert.doesNotMatch(flow, /Could not add Project/);
  assert.match(flow, /Repository .* added from local folder/);
});

test("the 30617 add flow never calls the repository a Project in user-visible copy", async () => {
  const dialogUrl = new URL(
    "./ui/crew-add-project-dialog.tsx",
    import.meta.url,
  );
  const libUrl = new URL(
    "./lib/project-add-local-workspace.ts",
    import.meta.url,
  );
  const runtimeUrl = new URL(
    "./lib/project-add-local-workspace-runtime.ts",
    import.meta.url,
  );
  const pickerUrl = new URL(
    "../../shared/api/tauri-project-folder-dialog.ts",
    import.meta.url,
  );
  const [dialog, lib, runtime, picker] = await Promise.all([
    readFile(dialogUrl, "utf8"),
    readFile(libUrl, "utf8"),
    readFile(runtimeUrl, "utf8"),
    readFile(pickerUrl, "utf8"),
  ]);

  assert.match(dialog, /Add this Repository\?/);
  assert.match(dialog, /Repository name/);
  assert.match(dialog, /"Add Repository"/);
  assert.match(dialog, /Add this Cowork Project\?/);
  assert.match(dialog, /Cowork Project name/);
  assert.match(dialog, /"Add Cowork Project"/);
  assert.match(lib, /Could not create Repository\./);
  assert.match(lib, /Repository name must include letters or numbers\./);
  assert.match(lib, /already have a Repository named/);
  assert.match(runtime, /Timed out adding the Repository\./);
  assert.match(runtime, /Failed to add the Repository\./);
  assert.match(picker, /Select Repository workspace/);
  // The whole chain: no user-visible string may call the 30617 entity a Project.
  for (const [name, source] of [
    ["dialog", dialog],
    ["lib", lib],
    ["runtime", runtime],
    ["picker", picker],
  ]) {
    const userStrings = source.match(/"[^"\n]*\bProject\b[^"\n]*"/g) ?? [];
    const offending = userStrings.filter(
      (s) =>
        !s.includes("ProjectLocalWorkspaceCreateError") &&
        !s.includes("Project channel") &&
        !s.includes("Cowork Project"),
    );
    assert.deepEqual(
      offending,
      [],
      `${name} still says Project in user-visible copy: ${offending.join(", ")}`,
    );
  }
});
