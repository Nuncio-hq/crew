import { type ReactNode, useEffect } from "react";

import { ProjectThreadWorkspacePanel } from "@/features/messages/ui/ProjectThreadWorkspacePanel";
import type { ProjectThreadWorkspaceModel } from "@/features/messages/ui/useProjectThreadWorkspaceModel";
import { setThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { useElementWidth, useIsThreadPanelOverlay } from "@/shared/hooks/use-mobile";
import { cn } from "@/shared/lib/cn";
import {
  getAuxiliaryPanelBodyClass,
  type AuxiliaryPanelMode,
} from "@/shared/layout/AuxiliaryPanel";
import { shouldStackDeclaredPlansRail } from "@/shared/layout/responsiveContract";

import { DeclaredPlansRail } from "./DeclaredPlansRail";
import { useDeclaredPlansForThread } from "./useDeclaredPlansForThread";

/**
 * Thread-panel body with declared-plans rail (#205).
 *
 * Auxiliary panel min 300px, max 720px. At ≤340px (and whenever a `w-72`
 * side rail would squeeze the transcript) the rail **stacks** between the
 * header and the scroll region — never overlaps chrome, never letter-soup.
 */
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
  const [bodyRef, paneWidthPx] = useElementWidth<HTMLDivElement>();
  const { plans } = useDeclaredPlansForThread({
    channelId,
    profiles,
    threadHead,
    threadMessages,
  });
  const showRail = !isHuddleTranscript && !isOverlay && plans.length > 0;
  const stacked =
    showRail &&
    (paneWidthPx === 0 || shouldStackDeclaredPlansRail(paneWidthPx));

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
        "@container flex min-h-0 min-w-0 flex-1",
        stacked ? "flex-col" : showRail ? "flex-row" : "flex-col",
        getAuxiliaryPanelBodyClass({ mode: panelChromeMode }),
      )}
      data-plans-layout={stacked ? "stacked" : showRail ? "side" : "none"}
      data-testid="declared-plans-body"
      ref={bodyRef}
    >
      {stacked ? (
        <DeclaredPlansRail layout="stacked" plans={plans} profiles={profiles} />
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
      {showRail && !stacked ? (
        <DeclaredPlansRail layout="side" plans={plans} profiles={profiles} />
      ) : null}
    </div>
  );
}
