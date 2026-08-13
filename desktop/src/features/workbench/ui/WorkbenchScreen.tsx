import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useWorkbenchThreadIndex } from "../hooks/useWorkbenchThreadIndex";
import type { WorkbenchLens } from "../lib/workbenchRoutes";
import {
  findWorkbenchRow,
  type WorkbenchThreadRow,
} from "../lib/workbenchThreadIndex";
import { WorkbenchEmptyState } from "./WorkbenchEmptyState";
import { WorkbenchRail } from "./WorkbenchRail";
import { WorkbenchThreadView } from "./WorkbenchThreadView";

export function WorkbenchScreen({
  channelId,
  lens,
  messageId,
  office,
  threadRootId,
}: {
  channelId?: string;
  lens: WorkbenchLens;
  messageId?: string;
  office: boolean;
  threadRootId?: string;
}) {
  const { goWorkbench, goChannel } = useAppNavigation();
  const index = useWorkbenchThreadIndex({ channelId, threadRootId });
  const selected = React.useMemo(() => {
    if (!channelId || !threadRootId) return null;
    return findWorkbenchRow(index.rows, channelId, threadRootId);
  }, [channelId, index.rows, threadRootId]);

  const onSelect = React.useCallback(
    (row: WorkbenchThreadRow) => {
      void goWorkbench(row.channelId, row.threadRootId, {
        lens,
        office,
        messageId: row.messageEventId,
      });
    },
    [goWorkbench, lens, office],
  );

  const onLensChange = React.useCallback(
    (next: WorkbenchLens) => {
      void goWorkbench(channelId, threadRootId, {
        lens: next,
        office,
        messageId,
      });
    },
    [channelId, goWorkbench, messageId, office, threadRootId],
  );

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 overflow-hidden"
      data-testid="workbench-screen"
    >
      <WorkbenchRail
        agentGroups={index.byAgent}
        channelGroups={index.byChannel}
        lens={lens}
        onLensChange={onLensChange}
        onSelect={onSelect}
        selectedThreadRootId={threadRootId}
      />
      {channelId && threadRootId ? (
        <WorkbenchThreadView
          channelId={channelId}
          officeView={office}
          onOpenChannel={() => {
            void goChannel(channelId, {
              thread: threadRootId,
              messageId,
              threadRootId,
            });
          }}
          onToggleOffice={() => {
            void goWorkbench(channelId, threadRootId, {
              lens,
              office: !office,
              messageId,
            });
          }}
          railRow={selected}
          threadRootId={threadRootId}
        />
      ) : (
        <WorkbenchEmptyState />
      )}
    </div>
  );
}
