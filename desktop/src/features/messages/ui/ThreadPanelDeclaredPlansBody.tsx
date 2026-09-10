import { type ReactNode, useEffect } from "react";

import { ProjectThreadWorkspacePanel } from "@/features/messages/ui/ProjectThreadWorkspacePanel";
import type { ProjectThreadWorkspaceModel } from "@/features/messages/ui/useProjectThreadWorkspaceModel";
import { setThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { cn } from "@/shared/lib/cn";
import {
  getAuxiliaryPanelBodyClass,
  type AuxiliaryPanelMode,
} from "@/shared/layout/AuxiliaryPanel";
import { openToolPane } from "@/features/tool-pane/toolPaneStore";
import { setThreadViewMode } from "@/features/channels/lib/threadViewModePreference";

/** Conversation body; declared plans live in the explicit Agent plans tab. */
export function ThreadPanelDeclaredPlansBody({
  channelId,
  children,
  isFocusMode,
  isHuddleTranscript,
  panelChromeMode,
  profiles,
  threadHead,
  threadMessages,
  workspaceModel,
}: {
  channelId: string | null;
  children: ReactNode;
  isFocusMode: boolean;
  isHuddleTranscript: boolean;
  panelChromeMode: AuxiliaryPanelMode;
  profiles?: UserProfileLookup;
  threadHead: TimelineMessage;
  threadMessages: TimelineMessage[];
  workspaceModel: ProjectThreadWorkspaceModel | null;
}) {
  useEffect(() => {
    setThreadForgeViewContext({
      channelId,
      rootEventId: threadHead.id,
      messages: [threadHead, ...threadMessages],
      profiles,
    });
    return () => {
      setThreadForgeViewContext(null);
    };
  }, [channelId, profiles, threadHead, threadMessages]);

  return (
    <div
      className={cn(
        "@container flex min-h-0 min-w-0 flex-1 flex-col",
        getAuxiliaryPanelBodyClass({ mode: panelChromeMode }),
      )}
      data-plans-layout="none"
      data-testid="declared-plans-body"
    >
      {!isHuddleTranscript ? (
        <button
          aria-label="Open thread tools"
          className="self-end rounded-md px-3 py-1 text-xs text-muted-foreground hover:bg-muted"
          onClick={() => {
            setThreadViewMode("focus");
            openToolPane();
          }}
          type="button"
        >
          Tools
        </button>
      ) : null}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <ProjectThreadWorkspacePanel
          channelId={channelId}
          isFocusMode={isFocusMode}
          model={workspaceModel}
          profiles={profiles}
        />
        {children}
      </div>
    </div>
  );
}
