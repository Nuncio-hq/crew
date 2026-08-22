import { basename, dirname, normalize } from "@tauri-apps/api/path";

import { getProjectLocalRepoSnapshot } from "@/shared/api/projectGit";
import type { ProjectLocalRepoSnapshot } from "@/shared/api/types";

type SnapshotInput = {
  baseBranch?: string | null;
  defaultBranch?: string | null;
  localWorkspacePath: string;
  projectDtag: string;
};

type SnapshotDependencies = {
  basename: (path: string) => Promise<string>;
  dirname: (path: string) => Promise<string>;
  getLocalSnapshot: typeof getProjectLocalRepoSnapshot;
  normalize: (path: string) => Promise<string>;
};

const runtimeDependencies: SnapshotDependencies = {
  basename,
  dirname,
  getLocalSnapshot: getProjectLocalRepoSnapshot,
  normalize,
};

export async function readExactLocalWorkspaceSnapshot(
  input: SnapshotInput,
  dependencies: SnapshotDependencies = runtimeDependencies,
): Promise<ProjectLocalRepoSnapshot | null> {
  const [reposDir, projectDtag, selectedPath] = await Promise.all([
    dependencies.dirname(input.localWorkspacePath),
    dependencies.basename(input.localWorkspacePath),
    dependencies.normalize(input.localWorkspacePath),
  ]);
  const result = await dependencies.getLocalSnapshot({
    baseBranch: input.baseBranch ?? null,
    cloneUrl: null,
    defaultBranch: input.defaultBranch ?? null,
    projectDtag,
    reposDir,
  });
  if (!result) return null;

  const resolvedPath = await dependencies.normalize(result.path);
  if (resolvedPath !== selectedPath) {
    throw new Error(
      "The linked workspace resolved to a different folder. Relink the Project.",
    );
  }
  return result;
}

export async function readProjectLocalRepoSnapshot(
  input: {
    baseBranch?: string | null;
    cloneUrl?: string | null;
    /**
     * Every clone URL the repository announces. A checkout's origin may be any
     * of them (a NIP-34 announcement can list a relay-hosted mirror beside the
     * canonical host), so each is tried in announced order.
     */
    cloneUrls?: readonly string[];
    defaultBranch?: string | null;
    localWorkspacePath?: string | null;
    localWorkspaceStatus?: "invalid" | "linked" | "unlinked";
    projectDtag: string;
    reposDir?: string | null;
    /** A selected tag is remote-only; see the fail-closed note below. */
    selectedTag?: string | null;
  },
  dependencies: SnapshotDependencies = runtimeDependencies,
) {
  if (input.localWorkspaceStatus === "invalid") return null;
  // The native reader only resolves branches. Reading a working copy while a
  // tag is selected would label branch-tip data as the tag, so decline.
  if (input.selectedTag) return null;
  if (input.localWorkspacePath) {
    return readExactLocalWorkspaceSnapshot(
      {
        baseBranch: input.baseBranch,
        defaultBranch: input.defaultBranch,
        localWorkspacePath: input.localWorkspacePath,
        projectDtag: input.projectDtag,
      },
      dependencies,
    );
  }
  const cloneUrlCandidates =
    input.cloneUrls && input.cloneUrls.length > 0
      ? input.cloneUrls
      : [input.cloneUrl ?? null];
  for (const cloneUrl of cloneUrlCandidates) {
    const result = await dependencies.getLocalSnapshot({
      baseBranch: input.baseBranch,
      cloneUrl,
      defaultBranch: input.defaultBranch,
      projectDtag: input.projectDtag,
      reposDir: input.reposDir,
    });
    if (result) return result;
  }
  return null;
}

export function localWorkspaceSourceState(input: {
  hasSnapshot: boolean;
  isError: boolean;
  isLinked: boolean;
  isLoading: boolean;
  isTagSelected?: boolean;
}) {
  // A tag is served from the remote repository only, so Local cannot be the
  // source even when a usable checkout exists.
  if (input.isTagSelected)
    return { disabled: true, label: "Local unavailable" };
  if (input.hasSnapshot) return { disabled: false, label: "Local" };
  if (input.isLoading) return { disabled: false, label: "Local checking" };
  return {
    disabled: true,
    label:
      input.isLinked || input.isError ? "Local unavailable" : "Local missing",
  };
}
