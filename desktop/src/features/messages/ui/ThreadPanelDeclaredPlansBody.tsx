import { type ReactNode, useEffect } from "react";

import { ProjectThreadWorkspacePanel } from "@/features/messages/ui/ProjectThreadWorkspacePanel";
import type { ProjectThreadWorkspaceModel } from "@/features/messages/ui/useProjectThreadWorkspaceModel";
import { setThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { useIsThreadPanelOverlay } from "@/shared/hooks/use-mobile";
import { cn } from "@/shared/lib/cn";
import {
  getAuxiliaryPanelBodyClass,
  type AuxiliaryPanelMode,
} from "@/shared/layout/AuxiliaryPanel";

import { DeclaredPlansRail } from "./DeclaredPlansRail";
import { useDeclaredPlansForThread } from "./useDeclaredPlansForThread";

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
  const isOverlay = useIsThreadPanelOverlay();
  const { plans } = useDeclaredPlansForThread({
    channelId,
    profiles,
    threadHead,
    threadMessages,
  });
  const showRail = !isHuddleTranscript && !isOverlay && plans.length > 0;

  useEffect(() => {
    setThreadForgeViewContext({
      channelId,
      rootEventId: threadHead.id,
      messages: [threadHead, ...threadMessages],
    });
    return () => {
      setThreadForgeViewContext(null);
    };
  }, [channelId, threadHead, threadMessages]);

  return (
    <div
      className={cn(
        "flex min-h-0 flex-1",
        showRail ? "flex-row" : "flex-col",
        getAuxiliaryPanelBodyClass({ mode: panelChromeMode }),
      )}
    >
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <ProjectThreadWorkspacePanel
          channelId={channelId}
          isFocusMode={isFocusMode}
          model={workspaceModel}
          profiles={profiles}
        />
        {children}
      </div>
      {showRail ? (
        <DeclaredPlansRail plans={plans} profiles={profiles} />
      ) : null}
    </div>
  );
}
