import { SidebarGroup, SidebarGroupLabel } from "@/shared/ui/sidebar";
import type {
  WorkThreadRowModel,
  WorkTreeFolderModel,
} from "../lib/workTreeTypes";
import { WorkTreeFolder } from "./WorkTreeFolder";

export function WorkTreeSection({
  folders,
  onSelectFolder,
  onSelectThread,
  selectedChannelId,
  selectedThreadRootId,
  selectedView,
}: {
  folders: readonly WorkTreeFolderModel[];
  onSelectFolder: (channelId: string) => void;
  onSelectThread: (row: WorkThreadRowModel) => void;
  selectedChannelId: string | null;
  selectedThreadRootId: string | null;
  selectedView: string;
}) {
  if (folders.length === 0) return null;
  return (
    <SidebarGroup className="p-0" data-testid="work-tree-projects">
      <SidebarGroupLabel className="px-2">Projects</SidebarGroupLabel>
      <div
        className="flex flex-col gap-0.5"
        data-testid="work-tree"
        role="tree"
      >
        {folders.map((folder) => (
          <WorkTreeFolder
            folder={folder}
            isActive={
              selectedView === "channel" &&
              selectedChannelId === folder.channelId
            }
            key={folder.channelId}
            onSelectFolder={onSelectFolder}
            onSelectThread={onSelectThread}
            selectedThreadRootId={
              selectedChannelId === folder.channelId
                ? selectedThreadRootId
                : null
            }
          />
        ))}
      </div>
    </SidebarGroup>
  );
}
