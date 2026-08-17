import * as React from "react";
import { useLocation } from "@tanstack/react-router";

import { selectedSessionFromLocation } from "@/features/workbench/lib/liveJobDesk";
import {
  useNeedsYouItems,
  useWorkTreeProjection,
} from "../hooks/useWorkTreeProjection";
import type { NeedsYouItem, WorkThreadRowModel } from "../lib/workTreeTypes";
import { NeedsYouSection } from "./NeedsYouSection";
import { WorkTreeSection } from "./WorkTreeSection";
import { useWorkTreeKeyboard } from "./useWorkTreeKeyboard";

export function WorkTreeSidebarBlock({
  onSelectFolder,
  onSelectThread,
  selectedChannelId,
  selectedView,
  unreadChannelIds,
}: {
  onSelectFolder: (channelId: string) => void;
  onSelectThread: (channelId: string, threadRootId: string) => void;
  selectedChannelId: string | null;
  selectedView: string;
  unreadChannelIds: ReadonlySet<string>;
}) {
  const location = useLocation();
  const session = selectedSessionFromLocation(location);
  const { folders } = useWorkTreeProjection(unreadChannelIds);
  const needsYou = useNeedsYouItems();
  const [needsYouOpen, setNeedsYouOpen] = React.useState(false);
  const treeRef = React.useRef<HTMLDivElement>(null);
  useWorkTreeKeyboard(treeRef);

  const handleSelectThread = React.useCallback(
    (row: WorkThreadRowModel) => {
      onSelectThread(row.channelId, row.threadRootId);
    },
    [onSelectThread],
  );

  const handleOpenNeedsYou = React.useCallback(
    (item: NeedsYouItem) => {
      onSelectThread(item.channelId, item.threadRootId);
    },
    [onSelectThread],
  );

  return (
    <div data-testid="work-tree-sidebar" ref={treeRef}>
      <NeedsYouSection
        count={needsYou.count}
        grouped={needsYou.grouped}
        onOpenItem={handleOpenNeedsYou}
        onToggle={() => setNeedsYouOpen((open) => !open)}
        open={needsYouOpen}
      />
      <WorkTreeSection
        folders={folders}
        onSelectFolder={onSelectFolder}
        onSelectThread={handleSelectThread}
        selectedChannelId={session.channelId ?? selectedChannelId}
        selectedThreadRootId={session.threadRootId}
        selectedView={selectedView}
      />
    </div>
  );
}
