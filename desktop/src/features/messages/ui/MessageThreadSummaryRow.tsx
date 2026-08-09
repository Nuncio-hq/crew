import * as React from "react";

import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import type {
  TimelineThreadSummary,
  TimelineThreadSummaryParticipant,
} from "@/features/messages/lib/threadPanel";
import type { ProjectThreadBadge } from "@/features/messages/lib/projectThreadBadge";
import { useProjectThreadBadge } from "@/features/messages/lib/useProjectThreadBadge";
import type { TimelineMessage } from "@/features/messages/types";
import type { ThreadDepthGuideAction } from "@/features/messages/ui/MessageRow";
import { formatThreadSummaryLastReplyTime } from "@/features/messages/lib/dateFormatters";
import {
  getThreadReplyAvatarCenterRem,
  getThreadReplyIndentRem,
  threadReplyLength,
  THREAD_REPLY_BODY_OFFSET_REM,
  THREAD_REPLY_LINE_WIDTH_REM,
  THREAD_REPLY_ROW_MARGIN_INLINE_REM,
} from "@/features/messages/lib/threadTreeLayout";
import { ProjectThreadBadgeChips } from "@/features/messages/ui/ProjectThreadBadgeChips";
import { ThreadAgentStatusChip } from "@/features/messages/ui/ThreadAgentStatusChip";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { cn } from "@/shared/lib/cn";
import { UserAvatar } from "@/shared/ui/UserAvatar";

const THREAD_SUMMARY_CONTENT_OFFSET_REM =
  THREAD_REPLY_BODY_OFFSET_REM - THREAD_REPLY_ROW_MARGIN_INLINE_REM;
const THREAD_SUMMARY_SURFACE_AVATAR_INSET_REM = 0.25;

function ParticipantAvatar({
  participant,
  index,
  participantCount,
}: {
  participant: TimelineThreadSummaryParticipant;
  index: number;
  participantCount: number;
}) {
  return (
    <div
      className={cn(
        index > 0 ? "-ml-1" : "",
        // S: cap at 2 avatars; XS: cap at 1. Hidden avatars stay in the DOM
        // for screen readers via the row aria-label / title elsewhere.
        index >= 2 && "[@container(max-width:519.9px)]:hidden",
        index >= 1 && "[@container(max-width:419.9px)]:hidden",
      )}
      data-testid="message-thread-summary-participant"
      style={{
        zIndex: index + 1,
        ...(index < participantCount - 1 && {
          mask: "radial-gradient(circle 14px at calc(100% + 6px) 50%, transparent 99%, #fff 100%)",
          WebkitMask:
            "radial-gradient(circle 14px at calc(100% + 6px) 50%, transparent 99%, #fff 100%)",
        }),
      }}
    >
      <UserAvatar
        avatarUrl={participant.avatarUrl}
        className="h-6 w-6 text-2xs"
        displayName={participant.author}
        size="sm"
      />
    </div>
  );
}

export function MessageThreadSummaryRow({
  badge: badgeProp,
  channelId,
  collapseDepthGuideActions,
  currentPubkey,
  depth = 0,
  depthGuideDepths,
  highlightThreadLineDepths,
  isActive = false,
  message,
  onCollapseDepthGuide,
  onCollapseDepthGuideHoverChange,
  onOpenThread,
  profiles,
  showDepthGuides = true,
  summary,
  summaryIndentOffsetRem = 0,
  unreadCount,
}: {
  badge?: ProjectThreadBadge | null;
  channelId?: string | null;
  collapseDepthGuideActions?: ReadonlyArray<ThreadDepthGuideAction>;
  currentPubkey?: string;
  depth?: number;
  depthGuideDepths?: ReadonlyArray<number>;
  highlightThreadLineDepths?: ReadonlyArray<number>;
  /** True when this summary's root is the open thread's timeline anchor. */
  isActive?: boolean;
  message: TimelineMessage;
  onCollapseDepthGuide?: (message: TimelineMessage) => void;
  onCollapseDepthGuideHoverChange?: (
    message: TimelineMessage,
    hovered: boolean,
  ) => void;
  onOpenThread: (message: TimelineMessage) => void;
  profiles?: UserProfileLookup;
  showDepthGuides?: boolean;
  summary: TimelineThreadSummary;
  summaryIndentOffsetRem?: number;
  unreadCount?: number;
}) {
  const derivedBadge = useProjectThreadBadge(
    badgeProp === undefined ? message : null,
  );
  const badge = badgeProp === undefined ? derivedBadge : badgeProp;
  // ACP conversation ids are derived from channel + thread-head event id —
  // same seam as composer bot activity and pending agent requests.
  const conversationId = React.useMemo(
    () => deriveAgentConversationIdOrNull(channelId, message.id),
    [channelId, message.id],
  );
  const indentRem = getThreadReplyIndentRem(depth);
  const hoverLeftRem =
    indentRem + THREAD_REPLY_ROW_MARGIN_INLINE_REM + summaryIndentOffsetRem;
  const hoverLeft = threadReplyLength(hoverLeftRem);
  const contentPaddingStart = threadReplyLength(
    THREAD_SUMMARY_CONTENT_OFFSET_REM,
  );
  const surfaceInsetStart = `calc(${contentPaddingStart} - ${threadReplyLength(
    THREAD_SUMMARY_SURFACE_AVATAR_INSET_REM,
  )})`;
  const replyLabel = summary.replyCount === 1 ? "reply" : "replies";
  const lastReplyLabel = summary.lastReplyAt
    ? `last reply ${formatThreadSummaryLastReplyTime(summary.lastReplyAt)}`
    : null;
  const summaryAriaLabel = isActive
    ? summary.lastReplyAt
      ? `Viewing thread with ${summary.replyCount} ${replyLabel}, ${lastReplyLabel}`
      : `Viewing thread with ${summary.replyCount} ${replyLabel}`
    : summary.lastReplyAt
      ? `View thread with ${summary.replyCount} ${replyLabel}, ${lastReplyLabel}`
      : `View thread with ${summary.replyCount} ${replyLabel}`;
  const guideDepths = depthGuideDepths
    ? [...depthGuideDepths]
    : Array.from({ length: Math.max(0, depth - 1) }, (_, index) => index + 1);
  const depthGuideItems = guideDepths.map((guideDepth) => ({
    depth: guideDepth,
    offset: getThreadReplyAvatarCenterRem(guideDepth),
  }));
  const collapseDepthGuideActionsByDepth = new Map(
    collapseDepthGuideActions?.map((action) => [action.depth, action]) ?? [],
  );

  return (
    <div className="relative w-full [container-type:inline-size] pb-1 pt-0.5">
      {showDepthGuides && depthGuideItems.length > 0 ? (
        <div
          aria-hidden={
            collapseDepthGuideActionsByDepth.size > 0 ? undefined : true
          }
          className={cn(
            "absolute left-0",
            collapseDepthGuideActionsByDepth.size === 0 &&
              "pointer-events-none",
          )}
          style={{ bottom: "-0.25rem", top: "-0.25rem" }}
        >
          {depthGuideItems.map(({ depth: guideDepth, offset }) => {
            const collapseAction =
              collapseDepthGuideActionsByDepth.get(guideDepth);
            const isHighlighted =
              Boolean(collapseAction?.active) ||
              Boolean(highlightThreadLineDepths?.includes(guideDepth));
            if (collapseAction) {
              return (
                <React.Fragment
                  key={`${message.id}-summary-depth-guide-${offset}`}
                >
                  <div
                    aria-hidden
                    className={cn(
                      "pointer-events-none absolute bottom-0 top-0 border-l transition-[border-color]",
                      isHighlighted ? "border-primary" : "border-border/45",
                    )}
                    style={{
                      borderLeftWidth: threadReplyLength(
                        THREAD_REPLY_LINE_WIDTH_REM,
                      ),
                      left: threadReplyLength(offset),
                    }}
                  />
                  <button
                    aria-label={collapseAction.label}
                    className="absolute bottom-0 top-0 z-20 w-5 -translate-x-1/2 cursor-pointer rounded-full focus-visible:outline-hidden"
                    data-thread-head-id={collapseAction.message.id}
                    data-testid="thread-collapse-guide"
                    onBlur={() =>
                      onCollapseDepthGuideHoverChange?.(
                        collapseAction.message,
                        false,
                      )
                    }
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      onCollapseDepthGuide?.(collapseAction.message);
                    }}
                    onFocus={() =>
                      onCollapseDepthGuideHoverChange?.(
                        collapseAction.message,
                        true,
                      )
                    }
                    onMouseEnter={() =>
                      onCollapseDepthGuideHoverChange?.(
                        collapseAction.message,
                        true,
                      )
                    }
                    onMouseLeave={() =>
                      onCollapseDepthGuideHoverChange?.(
                        collapseAction.message,
                        false,
                      )
                    }
                    style={{ left: threadReplyLength(offset) }}
                    type="button"
                  />
                </React.Fragment>
              );
            }

            return (
              <div
                aria-hidden
                className={cn(
                  "pointer-events-none absolute bottom-0 top-0 border-l transition-[border-color]",
                  isHighlighted ? "border-primary" : "border-border/45",
                )}
                key={`${message.id}-summary-depth-guide-${offset}`}
                style={{
                  borderLeftWidth: threadReplyLength(
                    THREAD_REPLY_LINE_WIDTH_REM,
                  ),
                  left: threadReplyLength(offset),
                }}
              />
            );
          })}
        </div>
      ) : null}

      <button
        aria-label={summaryAriaLabel}
        className={cn(
          "group relative isolate inline-flex min-h-[1.875rem] w-fit max-w-full cursor-pointer items-center gap-1.5 rounded-full py-0 pr-3 text-left text-xs font-medium transition-[color,opacity] focus-visible:outline-hidden",
          isActive
            ? "text-foreground"
            : "text-muted-foreground hover:text-foreground hover:opacity-90",
        )}
        data-thread-head-id={message.id}
        data-testid="message-thread-summary"
        onClick={() => onOpenThread(message)}
        style={{
          marginLeft: hoverLeft,
          maxWidth: `calc(100% - ${hoverLeft})`,
          paddingLeft: contentPaddingStart,
        }}
        type="button"
      >
        <span
          aria-hidden="true"
          className={cn(
            "pointer-events-none absolute bottom-[-0.125rem] top-[-0.125rem] rounded-full ring-border/70 transition-[background-color,box-shadow,opacity]",
            isActive
              ? "bg-background/95 opacity-100 ring-1"
              : "opacity-0 group-hover:bg-background/95 group-hover:opacity-100 group-hover:ring-1 group-focus-visible:bg-background/95 group-focus-visible:opacity-100 group-focus-visible:ring-1 group-focus-visible:ring-ring",
          )}
          data-testid="message-thread-summary-surface"
          style={{
            left: surfaceInsetStart,
            right: 0,
          }}
        />
        <div className="relative z-10 flex shrink-0 items-center">
          {summary.participants.map((participant, index) => (
            <ParticipantAvatar
              index={index}
              key={participant.id}
              participant={participant}
              participantCount={summary.participants.length}
            />
          ))}
        </div>
        <div className="relative z-10 min-w-0">
          <div className="flex flex-wrap items-center">
            <span
              className={cn(
                "font-medium transition-colors",
                !isActive && "group-hover:text-foreground",
              )}
              title={`${summary.replyCount} ${replyLabel}`}
            >
              {summary.replyCount}
              <span className="[@container(max-width:519.9px)]:hidden">
                {" "}
                {replyLabel}
              </span>
            </span>
            {unreadCount != null && unreadCount > 0 ? (
              <span className="ml-1" data-testid="thread-unread-badge">
                ({unreadCount} new)
              </span>
            ) : null}
            {summary.lastReplyAt || isActive ? (
              <span
                className="[@container(max-width:659.9px)]:hidden"
                data-testid="message-thread-summary-last-reply-wrap"
                title={
                  lastReplyLabel ?? (isActive ? "Viewing thread" : undefined)
                }
              >
                {summary.lastReplyAt ? (
                  <>
                    <span className="mx-1 font-normal text-muted-foreground/50">
                      ·
                    </span>
                    {isActive ? (
                      <span
                        className="font-normal text-muted-foreground/70"
                        data-testid="message-thread-summary-hover-action"
                      >
                        Viewing thread
                      </span>
                    ) : (
                      <span className="inline-grid font-normal text-muted-foreground/70">
                        <span
                          className="col-start-1 row-start-1 transition-opacity group-hover:opacity-0 group-focus-visible:opacity-0"
                          data-testid="message-thread-summary-last-reply"
                        >
                          last reply{" "}
                          {formatThreadSummaryLastReplyTime(
                            summary.lastReplyAt,
                          )}
                        </span>
                        <span
                          className="col-start-1 row-start-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100"
                          data-testid="message-thread-summary-hover-action"
                        >
                          View thread
                        </span>
                      </span>
                    )}
                  </>
                ) : (
                  <>
                    <span className="mx-1 font-normal text-muted-foreground/50">
                      ·
                    </span>
                    <span
                      className="font-normal text-muted-foreground/70"
                      data-testid="message-thread-summary-hover-action"
                    >
                      Viewing thread
                    </span>
                  </>
                )}
              </span>
            ) : null}
            {badge ? <ProjectThreadBadgeChips badge={badge} /> : null}
            <ThreadAgentStatusChip
              conversationId={conversationId}
              currentPubkey={currentPubkey}
              profiles={profiles}
            />
          </div>
        </div>
      </button>
    </div>
  );
}
