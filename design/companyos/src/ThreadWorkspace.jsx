import { Icon } from "./ui";
export const prStatus = (pr) =>
  pr.state === "Merged"
    ? "merged"
    : pr.state === "Closed"
      ? "closed"
      : pr.isDraft || pr.state === "Draft"
        ? "draft"
        : pr.state === "Open"
          ? "open"
          : "unknown";
export const prIcon = (pr) =>
  ({ merged: "merge", closed: "pr-closed", draft: "pr-draft", open: "pr" })[
    prStatus(pr)
  ] || "info";
export function ThreadWorkspace({ thread, onOpen }) {
  if (!thread.workspace) return null;
  const w = thread.workspace;
  return (
    <div
      className="thread-workspace"
      aria-label={`Workspace for ${thread.title}`}
    >
      <div className="workspace-branch">
        <Icon name="branch" size={13} />
        <span title={w.branch}>{w.branch}</span>
        <span className="workspace-repo">{w.repo}</span>
      </div>
      <div className="workspace-checkout">
        <Icon name="folder" size={13} />
        {onOpen ? (
          <button title={w.path} onClick={onOpen}>
            {w.name}
          </button>
        ) : (
          <span title={w.path}>{w.name}</span>
        )}
        <span className="workspace-kind">
          {w.state === "removed" ? "Worktree removed" : "Worktree"}
        </span>
        {w.state !== "removed" && (
          <span
            className="local-diff"
            aria-label={`${w.additions} additions and ${w.deletions} deletions, local changes against HEAD`}
          >
            <span className="diff-add">+{w.additions}</span>
            <span className="diff-remove">−{w.deletions}</span>
            <span>local vs HEAD</span>
          </span>
        )}
      </div>
    </div>
  );
}
export function PRState({ pr }) {
  const state = prStatus(pr);
  return (
    <span className={`pr-state pr-${state}`}>
      <Icon name={prIcon(pr)} size={14} />
      {state[0].toUpperCase() + state.slice(1)}
    </span>
  );
}
