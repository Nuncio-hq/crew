import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { cn } from "@/shared/lib/cn";
import { AgentRuntimeDefaultAvatar } from "./AgentRuntimeDefaultAvatar";

type EmojiPreview = { emoji: string; color: string };

type AgentAvatarFaceProps = {
  assetLabel?: string;
  avatarUrl: string | null;
  className?: string;
  emojiPreview?: EmojiPreview | null;
  isCompact?: boolean;
  isRoundedSquare?: boolean;
  label: string;
  runtimeId?: string | null;
  /** Pixel size for the runtime-default / initials fallback. */
  size: number;
};

/**
 * Shared agent face: custom image/emoji, else runtime SVG mark, else initials.
 */
export function AgentAvatarFace({
  assetLabel = "avatar",
  avatarUrl,
  className,
  emojiPreview,
  isCompact = false,
  isRoundedSquare = false,
  label,
  runtimeId,
  size,
}: AgentAvatarFaceProps) {
  if (emojiPreview) {
    return (
      <div
        aria-label={`${label} ${assetLabel}`}
        className={cn(
          "relative flex h-full w-full shrink-0 items-center justify-center overflow-hidden shadow-xs transition-[background-color] duration-200 ease-out",
          isRoundedSquare
            ? isCompact
              ? "rounded-2xl"
              : "rounded-[2rem]"
            : "rounded-full",
          className,
        )}
        role="img"
        style={{ backgroundColor: emojiPreview.color }}
      >
        <span
          className={cn(
            "flex h-full w-full items-center justify-center leading-none",
            isCompact ? "text-2xl" : "text-[4rem]",
          )}
        >
          {emojiPreview.emoji}
        </span>
      </div>
    );
  }

  const trimmed = avatarUrl?.trim() || null;
  if (isRoundedSquare && trimmed) {
    return (
      <img
        alt={`${label} ${assetLabel}`}
        className={cn(
          "h-full w-full object-cover shadow-xs",
          isCompact ? "rounded-2xl" : "rounded-[2rem]",
          className,
        )}
        src={trimmed}
      />
    );
  }

  if (trimmed) {
    return (
      <ProfileAvatar
        avatarUrl={trimmed}
        className={cn(
          "h-full w-full",
          isCompact ? "text-base" : "text-4xl",
          className,
        )}
        label={label}
      />
    );
  }

  return (
    <AgentRuntimeDefaultAvatar
      className={cn("h-full w-full border-0 shadow-none", className)}
      label={label}
      runtimeId={runtimeId}
      size={size}
    />
  );
}
