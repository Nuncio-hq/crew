import { useComposerAgentStop } from "@/features/channels/ui/useComposerAgentStop";
import { THREAD_PANEL_MESSAGE_GUTTER_CLASS } from "@/features/messages/lib/messageThreadPanelLayout";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { useLiveJobDesk } from "../hooks/useLiveJobDesk";

export function LiveJobDesk({
  channelId,
  threadRootId,
}: {
  channelId: string;
  threadRootId: string;
}) {
  const desk = useLiveJobDesk({ channelId, threadRootId });
  const { stopAgent } = useComposerAgentStop({
    channelId,
    conversationId: desk.conversationId,
  });

  if (!desk.show) return null;

  return (
    <div
      className={cn(
        THREAD_PANEL_MESSAGE_GUTTER_CLASS,
        "flex flex-wrap items-center gap-2 pb-2",
      )}
      data-testid="live-job-desk"
    >
      <p className="min-w-0 flex-1 text-xs text-muted-foreground">
        {desk.targetName} is working — steer if stuck
      </p>
      <Button
        data-testid="live-job-desk-steer"
        onClick={() => {
          const editor = document.querySelector(
            "[data-testid='thread-composer-overlay'] [contenteditable='true']",
          );
          if (editor instanceof HTMLElement) editor.focus();
        }}
        size="sm"
        type="button"
        variant="ghost"
      >
        Steer {desk.targetName}
      </Button>
      <Button
        data-testid="live-job-desk-stop"
        disabled={!desk.targetPubkey}
        onClick={() => {
          if (desk.targetPubkey) {
            void stopAgent(desk.targetPubkey, desk.targetName);
          }
        }}
        size="sm"
        type="button"
        variant="outline"
      >
        Stop {desk.targetName}
      </Button>
    </div>
  );
}
