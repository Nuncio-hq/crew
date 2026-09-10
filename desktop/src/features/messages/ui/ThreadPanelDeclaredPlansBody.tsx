import { type ReactNode, useEffect, useMemo, useRef } from "react";

import { useLoadArchivedObserverEvents } from "@/features/agents/ui/useObserverEvents";
import { setThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import { ProjectThreadWorkspacePanel } from "@/features/messages/ui/ProjectThreadWorkspacePanel";
import type { ProjectThreadWorkspaceModel } from "@/features/messages/ui/useProjectThreadWorkspaceModel";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { cn } from "@/shared/lib/cn";
import {
  getAuxiliaryPanelBodyClass,
  type AuxiliaryPanelMode,
} from "@/shared/layout/AuxiliaryPanel";
import {
  openThreadToolPane,
  useToolPane,
} from "@/features/tool-pane/toolPaneStore";

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
  const toolPane = useToolPane();
  const archiveScope = channelId;
  const archiveRequestRef = useRef({ requested: false, scope: archiveScope });
  if (archiveRequestRef.current.scope !== archiveScope) {
    archiveRequestRef.current = { requested: false, scope: archiveScope };
  }
  if (
    workspaceModel?.activePubkey ||
    (toolPane.open && (toolPane.tab === "activity" || toolPane.tab === "plans"))
  ) {
    // Keep one mounted owner active after its first consumer appears. Disabling
    // it when the pane closes would cancel an eager hydration pass and a later
    // reopen would have to start again from a second paging state.
    archiveRequestRef.current.requested = true;
  }
  const loadedArchivePaging = useLoadArchivedObserverEvents(
    Boolean(channelId && archiveRequestRef.current.requested),
    channelId,
  );
  const { fetchOlderArchived, hasOlderArchived } = loadedArchivePaging;
  const archivePaging = useMemo(
    () => ({
      fetchOlderArchived,
      hasOlderArchived,
    }),
    [fetchOlderArchived, hasOlderArchived],
  );

  useEffect(() => {
    setThreadForgeViewContext({
      archivePaging,
      channelId,
      rootEventId: threadHead.id,
      messages: [threadHead, ...threadMessages],
      profiles,
    });
  }, [archivePaging, channelId, profiles, threadHead, threadMessages]);

  useEffect(
    () => () => {
      setThreadForgeViewContext(null);
    },
    [],
  );

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
            openThreadToolPane();
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
