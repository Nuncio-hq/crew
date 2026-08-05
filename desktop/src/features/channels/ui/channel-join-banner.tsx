import { Hash, LogIn } from "lucide-react";

import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";

export function ChannelJoinBanner({
  activeChannel,
  isJoining,
  onJoinChannel,
}: {
  activeChannel: Channel;
  isJoining: boolean;
  onJoinChannel?: () => void | Promise<void>;
}) {
  return (
    <div
      data-testid="join-banner"
      className="flex items-center gap-3 border-t border-border/80 bg-card/50 px-5 py-3"
    >
      <div className="flex min-w-0 flex-1 items-center gap-2 text-sm text-muted-foreground">
        <Hash className="h-4 w-4 shrink-0" />
        <span className="truncate">
          Viewing{" "}
          <span className="font-medium text-foreground">
            #{activeChannel.name}
          </span>
        </span>
      </div>
      <Button
        disabled={isJoining}
        onClick={() => void onJoinChannel?.()}
        size="sm"
        variant="default"
      >
        <LogIn className="mr-1.5 h-4 w-4" />
        {isJoining ? "Joining..." : "Join to participate"}
      </Button>
    </div>
  );
}
