import * as React from "react";
import { useLocation } from "@tanstack/react-router";

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
  const workbenchMatch = location.pathname.match(
    /^\/workbench\/([^/]+)\/([^/]+)/,
  );
  const workbenchChannelId = workbenchMatch
    ? decodeURIComponent(workbenchMatch[1] ?? "")
    : null;
  const selectedThreadRootId = workbenchMatch
    ? decodeURIComponent(workbenchMatch[2] ?? "")
    : null;
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
        selectedChannelId={workbenchChannelId ?? selectedChannelId}
        selectedThreadRootId={selectedThreadRootId}
        selectedView={selectedView}
      />
    </div>
  );
}
