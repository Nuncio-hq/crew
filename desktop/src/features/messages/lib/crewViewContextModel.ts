import type {
  CrewViewContext,
  CrewViewSelectionItem,
} from "@/features/projects/lib/project-view-agent-context";

/** Minimal shape Crew needs from a thread's GitHub pull request. */
type ViewPullRequest = { number: number; title: string };

function channelItem(
  channelId: string,
  channelName: string,
): CrewViewSelectionItem {
  return { id: channelId, kind: "channel", title: `#${channelName}` };
}

/**
 * Visible-page context for thread focus mode: the thread the sender is reading
 * plus the workspace entities Crew already shows in that thread's chrome
 * (repository/branch from the worktree target, GitHub PR from the PR hub).
 * Nothing here is inferred — absent chrome means absent selection.
 *
 * Returns null unless that chrome contributes a selection the agent cannot
 * already derive from the message it receives: a bare channel or thread reply
 * stays byte-identical to what the sender typed.
 */
export function threadViewContext({
  branch,
  channelId,
  channelName,
  pullRequest,
  repositoryPath,
  threadRootId,
  threadTitle,
}: {
  branch: string | null;
  channelId: string | null;
  channelName: string | null;
  pullRequest: ViewPullRequest | null;
  repositoryPath: string | null;
  threadRootId: string | null;
  threadTitle: string;
}): CrewViewContext | null {
  if (!threadRootId || !channelId || !channelName) return null;
  if (!(repositoryPath && branch) && !pullRequest) return null;
  const selection: CrewViewSelectionItem[] = [];
  if (threadTitle.trim()) {
    selection.push({ id: threadRootId, kind: "task", title: threadTitle });
  }
  selection.push(channelItem(channelId, channelName));
  if (repositoryPath && branch) {
    const name = repositoryPath.split("/").filter(Boolean).at(-1) ?? "";
    selection.push({
      id: `${repositoryPath}#${branch}`,
      kind: "repository",
      title: `${name} on ${branch}`,
    });
  }
  if (pullRequest) {
    selection.push({
      id: `pr-${pullRequest.number}`,
      kind: "review",
      title: `PR #${pullRequest.number} ${pullRequest.title}`,
    });
  }
  return { scope: "thread", selection, view: `Thread in #${channelName}` };
}
