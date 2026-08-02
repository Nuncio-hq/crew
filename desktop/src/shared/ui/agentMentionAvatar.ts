import type { CSSProperties } from "react";

import { getAvatarSnapshotUrl } from "@/shared/lib/animatedAvatar";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { AGENT_MENTION_AVATAR_CLASS } from "@/shared/ui/mentionChip";

/** CSS custom property holding the avatar `url("…")` for the chip `::before`. */
export const AGENT_MENTION_AVATAR_VAR = "--agent-mention-avatar";

function escapeCssUrl(url: string): string {
  return url
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\r/g, "")
    .replace(/\n/g, "");
}

/**
 * Build the modifier class + inline style for an agent mention chip that
 * should paint an avatar instead of the robot glyph. Returns `{}` when there
 * is no usable URL so the chip keeps its robot fallback.
 */
export function agentMentionAvatarStyle(avatarUrl: string | null | undefined): {
  className?: string;
  style?: CSSProperties;
} {
  const trimmed = avatarUrl?.trim();
  if (!trimmed) {
    return {};
  }

  const snapshot = getAvatarSnapshotUrl(trimmed) ?? trimmed;
  const rewritten = rewriteRelayUrl(snapshot);
  if (!rewritten) {
    return {};
  }

  return {
    className: AGENT_MENTION_AVATAR_CLASS,
    style: {
      [AGENT_MENTION_AVATAR_VAR]: `url("${escapeCssUrl(rewritten)}")`,
    } as CSSProperties,
  };
}

/**
 * Serialize {@link agentMentionAvatarStyle} into ProseMirror decoration attrs
 * (`class` suffix + CSS declaration string). Returns null when the chip
 * should keep the robot glyph.
 */
export function agentMentionAvatarDecoration(
  avatarUrl: string | null | undefined,
): { className: string; style: string } | null {
  const result = agentMentionAvatarStyle(avatarUrl);
  if (!result.className || !result.style) {
    return null;
  }
  const cssValue = (result.style as Record<string, string | undefined>)[
    AGENT_MENTION_AVATAR_VAR
  ];
  if (!cssValue) {
    return null;
  }
  return {
    className: result.className,
    style: `${AGENT_MENTION_AVATAR_VAR}:${cssValue}`,
  };
}
