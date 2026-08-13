import type { UserInputEvent } from "@/features/channels/lib/userInput";
import { deriveUserInputRootEventId } from "@/features/channels/lib/userInput";
import type { TranscriptItem } from "@/features/agents/ui/agentSessionTypes";
import type { TimelineMessage } from "@/features/messages/types";
import { getThreadReference } from "@/features/messages/lib/threading";

export type WorkbenchObserverBundle = {
  agentPubkey: string;
  items: readonly TranscriptItem[];
};

export type WorkbenchSleepWake = {
  agentPubkey: string;
  kind: "sleep" | "wake";
  label: string;
};

export type WorkbenchTranscriptRow =
  | {
      type: "message";
      id: string;
      createdAt: number;
      message: TimelineMessage;
    }
  | {
      type: "user-input";
      id: string;
      createdAt: number;
      item: UserInputEvent;
    }
  | {
      type: "observer";
      id: string;
      createdAt: number;
      agentPubkey: string;
      item: TranscriptItem;
    }
  | {
      type: "sleep-wake";
      id: string;
      createdAt: number;
      agentPubkey: string;
      kind: "sleep" | "wake";
      label: string;
    }
  | {
      type: "catch-up";
      id: "catch-up";
      createdAt: number;
    };

export function userInputBelongsToThread(
  event: UserInputEvent["event"],
  threadRootId: string,
): boolean {
  const derived = deriveUserInputRootEventId(event);
  if (derived) return derived === threadRootId;
  const thread = getThreadReference(event.tags);
  const root = thread.rootId ?? thread.parentId;
  return root === threadRootId;
}

export function observerBelongsToThread(
  item: TranscriptItem,
  channelId: string,
  conversationId: string | null,
): boolean {
  if (item.channelId && item.channelId !== channelId) return false;
  if (item.conversationId && conversationId) {
    return item.conversationId === conversationId;
  }
  return item.channelId === channelId;
}

function observerCreatedAt(item: TranscriptItem): number {
  const parsed = Date.parse(item.timestamp);
  if (Number.isFinite(parsed)) return Math.floor(parsed / 1000);
  return 0;
}

export function buildWorkbenchTranscript(args: {
  channelId: string;
  conversationId: string | null;
  catchUpAfterId: string | null;
  messages: readonly TimelineMessage[];
  observerByAgent: readonly WorkbenchObserverBundle[];
  sleepWake: readonly WorkbenchSleepWake[];
  threadRootId: string;
  userInputs: readonly UserInputEvent[];
}): WorkbenchTranscriptRow[] {
  const rows: WorkbenchTranscriptRow[] = [];

  for (const message of args.messages) {
    rows.push({
      type: "message",
      id: message.id,
      createdAt: message.createdAt,
      message,
    });
  }

  for (const item of args.userInputs) {
    if (!userInputBelongsToThread(item.event, args.threadRootId)) continue;
    rows.push({
      type: "user-input",
      id: item.event.id,
      createdAt: item.event.created_at,
      item,
    });
  }

  for (const bundle of args.observerByAgent) {
    for (const item of bundle.items) {
      if (!observerBelongsToThread(item, args.channelId, args.conversationId)) {
        continue;
      }
      rows.push({
        type: "observer",
        id: `${bundle.agentPubkey}:${item.id}`,
        createdAt: observerCreatedAt(item),
        agentPubkey: bundle.agentPubkey,
        item,
      });
    }
  }

  for (const line of args.sleepWake) {
    rows.push({
      type: "sleep-wake",
      id: `sleep-wake:${line.kind}:${line.agentPubkey}`,
      createdAt: Number.MAX_SAFE_INTEGER - (line.kind === "sleep" ? 1 : 0),
      agentPubkey: line.agentPubkey,
      kind: line.kind,
      label: line.label,
    });
  }

  rows.sort((left, right) =>
    left.createdAt !== right.createdAt
      ? left.createdAt - right.createdAt
      : left.id.localeCompare(right.id),
  );

  if (!args.catchUpAfterId) return rows;

  const index = rows.findIndex(
    (row) => row.type === "message" && row.message.id === args.catchUpAfterId,
  );
  if (index < 0) return rows;
  const createdAt = rows[index]?.createdAt ?? 0;
  const next = [...rows];
  next.splice(index, 0, {
    type: "catch-up",
    id: "catch-up",
    createdAt,
  });
  return next;
}
