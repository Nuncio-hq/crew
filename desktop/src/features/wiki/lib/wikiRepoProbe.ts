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

/**
 * Card notice for wiki generate outcomes.
 *
 * Pre-#222 stub: any `empty-repo` job is treated as an empty GitHub tree.
 * Tests in wikiRepoProbe.test.mjs must fail this until the probe is split.
 */
export function classifyWikiRepoProbe(input: {
  jobError?: string | null;
  localWorkspacePath?: string | null;
  localWorkspaceStatus?: "invalid" | "linked" | "unlinked";
  remoteBranch?: string | null;
  remoteCommit?: string | null;
}): WikiRepoProbe {
  void input.localWorkspacePath;
  void input.localWorkspaceStatus;
  void input.remoteBranch;
  void input.remoteCommit;
  if (input.jobError === "empty-repo") {
    return {
      kind: "empty-tree",
      copy: WIKI_EMPTY_REPO_COPY,
      showGenerate: false,
    };
  }
  return { kind: "ok", copy: null, showGenerate: true };
}
