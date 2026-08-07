import type { ProjectThreadWorkspaceSnapshot } from "@/features/agents/projectThreadWorkspaceStore";
import { mergeObserverEventWindows } from "@/features/agents/ui/agentSessionPanelLayout";
import { getActivityHeadline } from "@/features/agents/ui/agentSessionTranscriptPresentation";
import type {
  ObserverEvent,
  TranscriptItem,
} from "@/features/agents/ui/agentSessionTypes";
import type { ProjectThreadAgentStep } from "@/features/messages/lib/projectThreadWorkspace";
import {
  ciStatus,
  pullRequestStatus,
} from "@/features/messages/lib/projectThreadGitHubStatus";
import type { ThreadPullRequest } from "@/shared/api/thread-workspace-types";

export type ProjectThreadPhaseState =
  | "complete"
  | "active"
  | "failed"
  | "waiting-on-user"
  | "pending";

export type ProjectThreadPhaseStates = {
  task: ProjectThreadPhaseState;
  workspace: ProjectThreadPhaseState;
  handoff: ProjectThreadPhaseState;
  pr: ProjectThreadPhaseState;
  ci: ProjectThreadPhaseState;
};

export function deriveProjectThreadPhaseStates({
  hasThread,
  pullRequest,
  steps,
  waitingOnUser = false,
  workspace,
}: {
  hasThread: boolean;
  pullRequest: ThreadPullRequest | null;
  steps: readonly ProjectThreadAgentStep[];
  waitingOnUser?: boolean;
  workspace: ProjectThreadWorkspaceSnapshot;
}): ProjectThreadPhaseStates {
  const workspaceState: ProjectThreadPhaseState =
    workspace.status === "error"
      ? "failed"
      : workspace.status === "ready" || workspace.status === "derived"
        ? "complete"
        : "active";

  const allStepsTerminal =
    steps.length > 0 && steps.every((step) => step.status === "done");
  const hasQueuedOrWorking = steps.some(
    (step) => step.status === "queued" || step.status === "working",
  );
  const handoffState: ProjectThreadPhaseState = waitingOnUser
    ? "waiting-on-user"
    : allStepsTerminal
      ? "complete"
      : hasQueuedOrWorking
        ? "active"
        : "pending";

  let prState: ProjectThreadPhaseState = "pending";
  let ciState: ProjectThreadPhaseState = "pending";
  if (pullRequest) {
    const prTone = pullRequestStatus(pullRequest).tone;
    prState =
      prTone === "merged"
        ? "complete"
        : prTone === "closed"
          ? "failed"
          : "active";

    if (pullRequest.checks.length > 0) {
      const ciTone = ciStatus(pullRequest.checks).tone;
      ciState =
        ciTone === "failure"
          ? "failed"
          : ciTone === "success"
            ? "complete"
            : "active";
    }
  }

  return {
    task: hasThread ? "complete" : "pending",
    workspace: workspaceState,
    handoff: handoffState,
    pr: prState,
    ci: ciState,
  };
}

export type ProjectThreadPeekFeedItem =
  | {
      id: string;
      kind: "thinking";
      text: string;
    }
  | {
      id: string;
      kind: "tool";
      headline: string;
      result: string | null;
      failed: boolean;
    };

const PROJECT_THREAD_PEEK_ITEM_CAP = 50;

export function mapProjectThreadPeekFeedItems(
  transcript: readonly TranscriptItem[],
): ProjectThreadPeekFeedItem[] {
  const feed: ProjectThreadPeekFeedItem[] = [];
  for (const item of transcript) {
    if (item.type === "thought") {
      const text = item.text.trim();
      if (text) feed.push({ id: item.id, kind: "thinking", text });
      continue;
    }
    if (item.type !== "tool" || item.renderClass === "suppressed") continue;
    feed.push({
      id: item.id,
      kind: "tool",
      headline: getActivityHeadline(item) ?? item.title,
      result: item.result.trim() || null,
      failed: item.isError || item.status === "failed",
    });
  }
  return feed.slice(-PROJECT_THREAD_PEEK_ITEM_CAP);
}

export function createProjectThreadPeekFeedSelector() {
  let previousTranscript: readonly TranscriptItem[] | null = null;
  let previousFeed: ProjectThreadPeekFeedItem[] = [];
  return (transcript: readonly TranscriptItem[]) => {
    if (transcript === previousTranscript) return previousFeed;
    previousTranscript = transcript;
    const nextFeed = mapProjectThreadPeekFeedItems(transcript);
    if (
      nextFeed.length === previousFeed.length &&
      nextFeed.every((item, index) =>
        projectThreadPeekFeedItemsEqual(item, previousFeed[index]),
      )
    ) {
      return previousFeed;
    }
    previousFeed = nextFeed;
    return previousFeed;
  };
}

function projectThreadPeekFeedItemsEqual(
  left: ProjectThreadPeekFeedItem,
  right: ProjectThreadPeekFeedItem | undefined,
): boolean {
  if (!right || left.id !== right.id || left.kind !== right.kind) return false;
  if (left.kind === "thinking" && right.kind === "thinking") {
    return left.text === right.text;
  }
  if (left.kind === "tool" && right.kind === "tool") {
    return (
      left.headline === right.headline &&
      left.result === right.result &&
      left.failed === right.failed
    );
  }
  return false;
}

export function mergeProjectThreadPeekEvents(
  liveEvents: readonly ObserverEvent[],
  archivedEvents: readonly ObserverEvent[],
  conversationId: string | null,
): ObserverEvent[] {
  if (!conversationId) return [];
  return mergeObserverEventWindows(liveEvents, archivedEvents).filter(
    (event) => event.conversationId === conversationId,
  );
}

export type ProjectThreadPeekMode = "live" | "history" | "hidden";

export function resolveProjectThreadPeekMode(
  active: boolean,
  itemCount: number,
): ProjectThreadPeekMode {
  if (active) return "live";
  return itemCount > 0 ? "history" : "hidden";
}

export function getProjectThreadPeekHeadline(
  transcript: readonly TranscriptItem[],
): string | null {
  for (let index = transcript.length - 1; index >= 0; index -= 1) {
    const item = transcript[index];
    if (item.type !== "thought" && item.type !== "tool") continue;
    const headline = getActivityHeadline(item);
    if (headline) return headline;
  }
  return null;
}
