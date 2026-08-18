/** Existing empty-tree sentence. Do not use this for a missing local folder. */
export const WIKI_EMPTY_REPO_COPY =
  "Empty repo / no default branch. Push to main, then Generate.";

/** Existing Projects copy for an unbound checkout. */
export const WIKI_NO_LOCAL_CHECKOUT_COPY = "No local checkout found.";

/** Existing #217 copy when a bound Project folder is gone. */
export const WIKI_GONE_FOLDER_COPY =
  "The Project folder is gone. Pick a workspace again.";

export type WikiRepoProbeKind =
  | "ok"
  | "empty-tree"
  | "missing-local"
  | "missing-local-gone";

export type WikiRepoProbe = {
  kind: WikiRepoProbeKind;
  copy: string | null;
  showGenerate: boolean;
};

function isUnbound(input: {
  localWorkspacePath?: string | null;
  localWorkspaceStatus?: "invalid" | "linked" | "unlinked";
}): boolean {
  return !input.localWorkspacePath || input.localWorkspaceStatus === "unlinked";
}

function remoteHasDefaultBranch(input: {
  remoteBranch?: string | null;
  remoteCommit?: string | null;
}): boolean {
  return Boolean(input.remoteCommit) || Boolean(input.remoteBranch);
}

/**
 * Card notice for wiki generate outcomes.
 *
 * `empty-repo` from the worker is only an empty local git tree. A missing
 * or gone checkout must not use the empty-GitHub sentence when the remote
 * already has a default branch.
 */
export function classifyWikiRepoProbe(input: {
  jobError?: string | null;
  localWorkspacePath?: string | null;
  localWorkspaceStatus?: "invalid" | "linked" | "unlinked";
  remoteBranch?: string | null;
  remoteCommit?: string | null;
}): WikiRepoProbe {
  const unbound = isUnbound(input);
  const remoteLive = remoteHasDefaultBranch(input);
  const emptyJob = input.jobError === "empty-repo";
  const missingJob = input.jobError === "missing-local-path";

  if (missingJob || (emptyJob && (unbound || remoteLive))) {
    if (unbound) {
      return {
        kind: "missing-local",
        copy: WIKI_NO_LOCAL_CHECKOUT_COPY,
        showGenerate: true,
      };
    }
    return {
      kind: "missing-local-gone",
      copy: WIKI_GONE_FOLDER_COPY,
      showGenerate: true,
    };
  }

  if (emptyJob) {
    return {
      kind: "empty-tree",
      copy: WIKI_EMPTY_REPO_COPY,
      showGenerate: false,
    };
  }

  return { kind: "ok", copy: null, showGenerate: true };
}
