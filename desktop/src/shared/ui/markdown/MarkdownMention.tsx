import { AgentManagementMarker } from "@/features/agents/ui/OtherSetupAgentMarker";
import type * as React from "react";

import { UserProfilePopover } from "@/features/profile/ui/UserProfilePopover";
import { agentMentionAvatarStyle } from "@/shared/ui/agentMentionAvatar";
import {
  MENTION_CHIP_BASE_CLASSES,
  MENTION_CHIP_HOVER_CLASSES,
  MENTION_CHIP_PREFIX_CLASS,
} from "@/shared/ui/mentionChip";
import { cn } from "@/shared/lib/cn";
import { useMarkdownRuntime } from "./runtimeContext";

export function MarkdownMention({
  children,
  interactive,
}: {
  children?: React.ReactNode;
  interactive: boolean;
}) {
  const {
    agentMentionAvatarsByName,
    agentMentionPubkeysByName,
    mentionPubkeysByName,
  } = useMarkdownRuntime();
  const mentionText = String(children ?? "");
  const mentionName = mentionText.replace(/^@/, "").trim().toLowerCase();
  const pubkey = mentionPubkeysByName?.[mentionName];
  const isAgentMention =
    pubkey !== undefined && agentMentionPubkeysByName?.[mentionName] === pubkey;
  const mentionLabel = mentionText.replace(/^@/, "");
  const avatar = isAgentMention
    ? agentMentionAvatarStyle(agentMentionAvatarsByName?.[mentionName])
    : {};
  const renderedMentionText = isAgentMention ? (
    mentionLabel
  ) : (
    <>
      <span className={MENTION_CHIP_PREFIX_CLASS}>@</span>
      {mentionLabel}
    </>
  );
  // Only chips that actually open a profile get the clickable affordance.
  // A mention whose pubkey didn't resolve stays a plain chip — a pointer
  // cursor there promises a click that does nothing.
  const opensProfile = interactive && pubkey !== undefined;
  const mentionNode = (
    <span
      data-mention=""
      className={cn(
        MENTION_CHIP_BASE_CLASSES,
        opensProfile && "cursor-pointer",
        opensProfile && MENTION_CHIP_HOVER_CLASSES,
        isAgentMention && "agent-mention-highlight",
        avatar.className,
      )}
      style={avatar.style}
    >
      {renderedMentionText}
      {isAgentMention ? <AgentManagementMarker pubkey={pubkey} /> : null}
    </span>
  );

  return opensProfile ? (
    <UserProfilePopover
      botIdenticonValue={mentionLabel}
      pubkey={pubkey}
      role={isAgentMention ? "bot" : undefined}
      triggerElement="span"
    >
      {mentionNode}
    </UserProfilePopover>
  ) : (
    mentionNode
  );
}
