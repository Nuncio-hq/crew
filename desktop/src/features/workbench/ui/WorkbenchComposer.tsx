import * as React from "react";

import { useComposerAgentStop } from "@/features/channels/ui/useComposerAgentStop";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { guidedHandoverManagedAgent } from "@/shared/api/agentControl";
import { Button } from "@/shared/ui/button";
import {
  agentDisplayName,
  cycleComposerTarget,
  mentionPubkeysForTarget,
  resolveSendTarget,
  type ThreadAgentRef,
} from "../lib/workbenchComposerTarget";
import { MessageComposer } from "../lib/workbenchSharedRenderers";

export function WorkbenchComposer({
  agents,
  channelId,
  channelName,
  conversationId,
  disabled,
  isSending,
  onSend,
  onTargetChange,
  officeView,
  profiles,
  targetPubkey,
  threadRootId,
}: {
  agents: readonly ThreadAgentRef[];
  channelId: string;
  channelName: string;
  conversationId: string | null;
  disabled?: boolean;
  isSending: boolean;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onTargetChange: (pubkey: string | null) => void;
  officeView: boolean;
  profiles?: UserProfileLookup;
  targetPubkey: string | null;
  threadRootId: string;
}) {
  const { stopAgent } = useComposerAgentStop({
    channelId,
    conversationId,
  });
  const targetName = agentDisplayName(agents, targetPubkey);

  const handleSend = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
    ) => {
      const target = resolveSendTarget({
        agentPubkeys: agents.map((agent) => agent.pubkey),
        chipPubkey: targetPubkey,
        mentionPubkeys,
      });
      if (target && target !== targetPubkey) {
        onTargetChange(target);
      }
      await onSend(
        content,
        mentionPubkeysForTarget(mentionPubkeys, target),
        mediaTags,
      );
    },
    [agents, onSend, onTargetChange, targetPubkey],
  );

  const cycle = React.useCallback(() => {
    onTargetChange(cycleComposerTarget(agents, targetPubkey));
  }, [agents, onTargetChange, targetPubkey]);

  const onDockKeyDownCapture = React.useCallback(
    (event: React.KeyboardEvent) => {
      if (
        event.key !== "Tab" ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      cycle();
    },
    [cycle],
  );

  if (officeView) {
    return (
      <div
        className="shrink-0 border-t border-border/60 px-4 py-2 text-xs text-muted-foreground"
        data-testid="workbench-office-composer-hidden"
      >
        Session controls and targeting stay in the workbench view.
      </div>
    );
  }

  return (
    <fieldset
      aria-label="Thread composer"
      className="m-0 min-w-0 shrink-0 border-0 border-t border-border/60 p-0"
      data-testid="workbench-composer"
      onKeyDownCapture={onDockKeyDownCapture}
    >
      <div className="flex flex-wrap items-center gap-2 px-4 pt-2">
        <button
          className="rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-xs font-medium"
          data-testid="workbench-target-chip"
          onClick={cycle}
          type="button"
        >
          → {targetName}
        </button>
        <span className="text-2xs text-muted-foreground">
          Tab cycles · @ overrides
        </span>
      </div>
      <MessageComposer
        audienceContext={{
          type: "thread",
          threadRootId,
          initialAgentPubkeys: agents.map((agent) => agent.pubkey),
        }}
        channelId={channelId}
        channelName={channelName}
        disabled={disabled || isSending || !channelId}
        draftKey={`workbench:${threadRootId}`}
        isSending={isSending}
        layoutMode="standalone"
        onSend={handleSend}
        placeholder="Message this thread…"
        profiles={profiles}
        typingParentEventId={threadRootId}
        typingRootEventId={threadRootId}
      />
      <div className="flex flex-wrap gap-2 px-4 pb-3">
        <Button
          data-testid="workbench-stop"
          disabled={!targetPubkey}
          onClick={() => {
            if (targetPubkey) void stopAgent(targetPubkey, targetName);
          }}
          size="sm"
          type="button"
          variant="outline"
        >
          Stop {targetName}
        </Button>
        <Button
          data-testid="workbench-steer"
          onClick={() => {
            const editor = document.querySelector(
              "[data-testid='workbench-composer'] [contenteditable='true']",
            );
            if (editor instanceof HTMLElement) editor.focus();
          }}
          size="sm"
          type="button"
          variant="ghost"
        >
          Steer {targetName}
        </Button>
        <Button
          data-testid="workbench-new-session"
          disabled={!targetPubkey || !conversationId}
          onClick={() => {
            if (!targetPubkey || !conversationId) return;
            void guidedHandoverManagedAgent(
              targetPubkey,
              channelId,
              conversationId,
              {
                rootEventId: threadRootId,
              },
            );
          }}
          size="sm"
          type="button"
          variant="ghost"
        >
          New session
        </Button>
      </div>
    </fieldset>
  );
}
