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

export type ProjectThreadPeekToolStatus = "running" | "done" | "failed";

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
      status: ProjectThreadPeekToolStatus;
    };

const PROJECT_THREAD_PEEK_ITEM_CAP = 50;
/** Collapsed thinking / result preview before the founder expands. */
export const PROJECT_THREAD_PEEK_PREVIEW_CHARS = 140;

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
    const failed = item.isError || item.status === "failed";
    feed.push({
      id: item.id,
      kind: "tool",
      headline: getActivityHeadline(item) ?? item.title,
      result: item.result.trim() || null,
      failed,
      status: peekToolStatus(item.status, failed),
    });
  }
  return feed.slice(-PROJECT_THREAD_PEEK_ITEM_CAP);
}

function peekToolStatus(
  status: "executing" | "completed" | "failed" | "pending",
  failed: boolean,
): ProjectThreadPeekToolStatus {
  if (failed || status === "failed") return "failed";
  if (status === "completed") return "done";
  return "running";
}

/**
 * Turn wire dumps into readable peek text: pretty-print JSON, then expand
 * literal `\n` / `\t` when the blob is still a single escaped line.
 */
export function formatProjectThreadPeekText(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return raw;

  let text = trimmed;
  try {
    text = JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    // keep trimmed
  }

  if (!text.includes("\n") && /\\[ntr]/.test(text)) {
    text = text.replace(/\\n/g, "\n").replace(/\\t/g, "\t").replace(/\\r/g, "");
  }

  return text;
}

/** First line / char-clamped preview for collapsed peek rows. */
export function previewProjectThreadPeekText(
  raw: string,
  maxChars = PROJECT_THREAD_PEEK_PREVIEW_CHARS,
): { preview: string; truncated: boolean } {
  const formatted = formatProjectThreadPeekText(raw).trim();
  if (!formatted) return { preview: "", truncated: false };

  const firstLine =
    formatted
      .split("\n")
      .find((line) => line.trim())
      ?.trim() ?? "";
  const hasMoreLines =
    formatted.includes("\n") && formatted.trim() !== firstLine;

  if (firstLine.length <= maxChars && !hasMoreLines) {
    return { preview: firstLine, truncated: false };
  }
  if (firstLine.length <= maxChars) {
    return { preview: firstLine, truncated: true };
  }
  return {
    preview: `${firstLine.slice(0, Math.max(0, maxChars - 1)).trimEnd()}…`,
    truncated: true,
  };
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
      left.failed === right.failed &&
      left.status === right.status
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
