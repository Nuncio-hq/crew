import { invokeTauri } from "@/shared/api/tauri";

export type ProjectGitWorkspaceProbe = {
  isGit: boolean;
  defaultBranch: string | null;
  currentBranch: string | null;
  dirty: boolean;
  uncommittedCount: number;
  localBranches: string[];
  remoteBranches: string[];
};

type RawProjectGitWorkspaceProbe = {
  isGit?: boolean;
  is_git?: boolean;
  defaultBranch?: string | null;
  default_branch?: string | null;
  currentBranch?: string | null;
  current_branch?: string | null;
  dirty?: boolean;
  uncommittedCount?: number;
  uncommitted_count?: number;
  localBranches?: string[];
  local_branches?: string[];
  remoteBranches?: string[];
  remote_branches?: string[];
};

export async function probeProjectGitWorkspace(
  path: string,
): Promise<ProjectGitWorkspaceProbe> {
  const probe = await invokeTauri<RawProjectGitWorkspaceProbe>(
    "probe_project_git_workspace",
    { path },
  );
  return {
    isGit: probe.isGit ?? probe.is_git ?? false,
    defaultBranch: probe.defaultBranch ?? probe.default_branch ?? null,
    currentBranch: probe.currentBranch ?? probe.current_branch ?? null,
    dirty: probe.dirty ?? false,
    uncommittedCount: probe.uncommittedCount ?? probe.uncommitted_count ?? 0,
    localBranches: probe.localBranches ?? probe.local_branches ?? [],
    remoteBranches: probe.remoteBranches ?? probe.remote_branches ?? [],
  };
}
