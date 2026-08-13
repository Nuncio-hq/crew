import type {
  UserInputAnswers,
  UserInputEvent,
} from "@/features/channels/lib/userInput";
import { AgentSessionTranscriptVariantProvider } from "@/features/agents/ui/agentSessionTranscriptContext";
import { ACTIVITY_RENDER_CLASS_PRESENTERS } from "@/features/agents/ui/activityRenderClasses/TranscriptActivityItem";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { TimelineMessage } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import {
  ChannelUserInputCard,
  MessageRow,
  TranscriptActivityItem,
  UnreadDivider,
} from "../lib/workbenchSharedRenderers";
import {
  isOfficeVisibleRow,
  isRoleCheckObserverItem,
} from "../lib/workbenchOfficeFilter";
import type { WorkbenchTranscriptRow } from "../lib/workbenchTranscript";

export function WorkbenchTranscript({
  channelId,
  currentPubkey,
  errors,
  officeView,
  onReply,
  onSkip,
  onSubmit,
  onToggleReaction,
  profiles,
  rows,
  sendingRequestId,
  sentIds,
  threadHeadId,
}: {
  channelId: string;
  currentPubkey: string;
  errors: Record<string, string>;
  officeView: boolean;
  onReply?: (message: TimelineMessage) => void;
  onSkip: (item: UserInputEvent) => Promise<void>;
  onSubmit: (item: UserInputEvent, answers: UserInputAnswers) => Promise<void>;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  profiles?: UserProfileLookup;
  rows: readonly WorkbenchTranscriptRow[];
  sendingRequestId: string | null;
  sentIds: ReadonlySet<string>;
  threadHeadId: string;
}) {
  const visible = officeView
    ? rows.filter((row) => isOfficeVisibleRow(row, threadHeadId))
    : rows;

  return (
    <div
      className="min-h-0 flex-1 overflow-y-auto px-3 py-2"
      data-testid="workbench-transcript"
    >
      <AgentSessionTranscriptVariantProvider value="default">
        {visible.map((row) => (
          <WorkbenchTranscriptItem
            channelId={channelId}
            currentPubkey={currentPubkey}
            errors={errors}
            key={row.id}
            onReply={onReply}
            onSkip={onSkip}
            onSubmit={onSubmit}
            onToggleReaction={onToggleReaction}
            profiles={profiles}
            row={row}
            sendingRequestId={sendingRequestId}
            sentIds={sentIds}
          />
        ))}
      </AgentSessionTranscriptVariantProvider>
    </div>
  );
}

function WorkbenchTranscriptItem({
  channelId,
  currentPubkey,
  errors,
  onReply,
  onSkip,
  onSubmit,
  onToggleReaction,
  profiles,
  row,
  sendingRequestId,
  sentIds,
}: {
  channelId: string;
  currentPubkey: string;
  errors: Record<string, string>;
  onReply?: (message: TimelineMessage) => void;
  onSkip: (item: UserInputEvent) => Promise<void>;
  onSubmit: (item: UserInputEvent, answers: UserInputAnswers) => Promise<void>;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  profiles?: UserProfileLookup;
  row: WorkbenchTranscriptRow;
  sendingRequestId: string | null;
  sentIds: ReadonlySet<string>;
}) {
  switch (row.type) {
    case "catch-up":
      return (
        <div data-testid="workbench-catch-up">
          <UnreadDivider label="NEW since you were here" />
        </div>
      );
    case "message":
      return (
        <div className="py-1">
          <MessageRow
            actionBarPlacement="inside"
            channelId={channelId}
            currentPubkey={currentPubkey}
            layoutVariant="thread-reply"
            message={row.message}
            onReply={onReply}
            onToggleReaction={onToggleReaction}
            profiles={profiles}
            showDepthGuides={false}
          />
        </div>
      );
    case "user-input":
      return (
        <div className="py-2">
          <ChannelUserInputCard
            currentPubkey={currentPubkey}
            error={errors[row.item.event.id]}
            item={row.item}
            onSkip={onSkip}
            onSubmit={onSubmit}
            profiles={profiles}
            sending={sendingRequestId === row.item.event.id}
            sent={sentIds.has(row.item.event.id)}
          />
        </div>
      );
    case "observer": {
      if (isRoleCheckObserverItem(row.item)) {
        const title = "title" in row.item ? row.item.title : "";
        const text = "text" in row.item ? row.item.text : "";
        return (
          <p
            className="px-3 py-1 font-mono text-2xs text-muted-foreground"
            data-testid="workbench-role-check"
          >
            {title}
            {text ? ` · ${text}` : ""}
          </p>
        );
      }
      if (
        !("renderClass" in row.item) ||
        !ACTIVITY_RENDER_CLASS_PRESENTERS[row.item.renderClass]
      ) {
        return null;
      }
      const profile = profiles?.[row.agentPubkey];
      return (
        <div
          className={cn("border-l-2 border-primary/40 py-0.5 pl-2")}
          data-testid={`workbench-tool-row-${row.id}`}
        >
          <TranscriptActivityItem
            agentAvatarUrl={profile?.avatarUrl ?? null}
            agentName={profile?.displayName ?? row.agentPubkey.slice(0, 8)}
            agentPubkey={row.agentPubkey}
            item={row.item}
            profiles={profiles}
          />
        </div>
      );
    }
    case "sleep-wake":
      return (
        <p
          className="px-3 py-1 text-xs text-muted-foreground"
          data-testid={`workbench-sleep-wake-${row.kind}`}
        >
          {row.kind === "sleep" ? "🌙 " : "… "}
          {row.label}
        </p>
      );
    default: {
      const _exhaustive: never = row;
      return _exhaustive;
    }
  }
}
