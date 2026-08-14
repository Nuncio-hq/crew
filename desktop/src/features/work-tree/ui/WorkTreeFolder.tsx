import * as React from "react";

import { useNow } from "@/shared/lib/useNow";
import {
  getWorkTreeDisclosure,
  setWorkTreeDisclosure,
} from "../lib/workTreeDisclosure";
import type {
  WorkThreadRowModel,
  WorkTreeFolderModel,
} from "../lib/workTreeTypes";
import { WorkThreadRow } from "./WorkThreadRow";
import { WorkTreeFolderRow } from "./WorkTreeFolderRow";

export function WorkTreeFolder({
  folder,
  isActive,
  onSelectFolder,
  onSelectThread,
  selectedThreadRootId,
}: {
  folder: WorkTreeFolderModel;
  isActive: boolean;
  onSelectFolder: (channelId: string) => void;
  onSelectThread: (row: WorkThreadRowModel) => void;
  selectedThreadRootId: string | null;
}) {
  const now = useNow(30_000);
  const expanded = folder.expanded;

  const onToggle = React.useCallback(
    (channelId: string) => {
      setWorkTreeDisclosure(channelId, { expanded: !expanded });
    },
    [expanded],
  );

  React.useEffect(() => {
    if (!folder.autoCollapsed) return;
    if (getWorkTreeDisclosure(folder.channelId)?.expanded === false) return;
    setWorkTreeDisclosure(folder.channelId, { expanded: false });
  }, [folder.autoCollapsed, folder.channelId]);

  return (
    <div data-testid={`work-tree-folder-block-${folder.channelId}`}>
      <WorkTreeFolderRow
        expanded={expanded}
        folder={folder}
        isActive={isActive}
        now={now}
        onSelectFolder={onSelectFolder}
        onToggle={onToggle}
      />
      {expanded
        ? folder.visibleThreads.map((row) => (
            <div className="pl-5" key={row.threadRootId}>
              <WorkThreadRow
                now={now}
                onSelect={onSelectThread}
                row={row}
                selected={row.threadRootId === selectedThreadRootId}
              />
            </div>
          ))
        : null}
      {expanded && folder.hiddenCount > 0 ? (
        <button
          className="ml-5 rounded-md px-2 py-0.5 text-2xs text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground"
          data-testid={`work-tree-more-${folder.channelName}`}
          onClick={() =>
            setWorkTreeDisclosure(folder.channelId, { moreExpanded: true })
          }
          type="button"
        >
          …{folder.hiddenCount} more
        </button>
      ) : null}
    </div>
  );
}
