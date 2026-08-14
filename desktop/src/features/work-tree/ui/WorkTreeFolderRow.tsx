import { ChevronDown, ChevronRight, Folder } from "lucide-react";

import { SidebarResourceDots } from "@/features/tool-pane/SidebarResourceDots";
import { cn } from "@/shared/lib/cn";
import { setWorkTreeDisclosure } from "../lib/workTreeDisclosure";
import { formatFolderQuietAge } from "../lib/workTreeFormat";
import type {
  WorkTreeFolderBadge,
  WorkTreeFolderModel,
} from "../lib/workTreeTypes";

function FolderBadgeFace({ badge }: { badge: WorkTreeFolderBadge }) {
  if (badge.kind === "needs-you") {
    return (
      <span
        className="shrink-0 text-2xs tabular-nums"
        data-testid="work-tree-folder-badge-needs-you"
      >
        🟡
      </span>
    );
  }
  return (
    <span
      className="shrink-0 text-2xs tabular-nums text-muted-foreground"
      data-testid="work-tree-folder-badge-live"
    >
      {badge.count} 🟢
    </span>
  );
}

export function WorkTreeFolderRow({
  expanded,
  folder,
  isActive,
  now,
  onSelectFolder,
  onToggle,
}: {
  expanded: boolean;
  folder: WorkTreeFolderModel;
  isActive: boolean;
  now: number;
  onSelectFolder: (channelId: string) => void;
  onToggle: (channelId: string) => void;
}) {
  const quietAge = folder.autoCollapsed
    ? formatFolderQuietAge(folder.lastActivityAt, now)
    : null;
  return (
    <div
      className="flex min-w-0 items-center gap-0.5"
      data-pinned={folder.pinned ? "true" : "false"}
      data-testid={`work-tree-folder-${folder.channelName}`}
    >
      <button
        aria-expanded={expanded}
        aria-label={
          expanded
            ? `Collapse ${folder.channelName}`
            : `Expand ${folder.channelName}`
        }
        className="flex size-5 shrink-0 items-center justify-center rounded-sm text-sidebar-foreground/60 hover:bg-sidebar-accent hover:text-sidebar-foreground"
        data-testid={`work-tree-disclosure-${folder.channelName}`}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onToggle(folder.channelId);
        }}
        type="button"
      >
        {expanded ? (
          <ChevronDown className="size-3.5" />
        ) : (
          <ChevronRight className="size-3.5" />
        )}
      </button>
      <button
        className={cn(
          "flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1 py-1 text-left hover:bg-sidebar-accent",
          isActive && "bg-sidebar-accent font-medium",
        )}
        data-testid={`channel-${folder.channelName}`}
        onClick={() => onSelectFolder(folder.channelId)}
        onContextMenu={(event) => {
          event.preventDefault();
          setWorkTreeDisclosure(folder.channelId, { pinned: !folder.pinned });
        }}
        type="button"
      >
        <Folder className="size-3.5 shrink-0 text-sidebar-foreground/70" />
        <span className="min-w-0 flex-1 truncate text-sm">
          {folder.channelName}
        </span>
        {folder.badge ? <FolderBadgeFace badge={folder.badge} /> : null}
        {quietAge ? (
          <span className="shrink-0 text-2xs tabular-nums text-muted-foreground">
            {quietAge}
          </span>
        ) : null}
        {folder.timelineUnread ? (
          <span
            className="h-2 w-2 shrink-0 rounded-full bg-primary"
            data-testid={`channel-unread-dot-${folder.channelName}`}
          />
        ) : null}
        <SidebarResourceDots channelId={folder.channelId} />
      </button>
    </div>
  );
}
