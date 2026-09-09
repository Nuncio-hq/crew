import { Activity, BookOpen, ChevronDown, Hash } from "lucide-react";

import { FeatureGate } from "@/shared/features";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/** Workspace destinations remain reachable independently of Project membership. */
export function WorkspaceNavigationMenu({
  name,
  onBrowseChannels,
  onSelectWiki,
  onSelectPulse,
}: {
  name: string;
  onBrowseChannels: () => void;
  onSelectWiki: () => void;
  onSelectPulse: () => void;
}) {
  return (
    <div className="px-3 pt-3">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            aria-label="Open workspace menu"
            className="flex max-w-full items-center gap-2 rounded-md px-1 py-1 text-sm font-semibold text-sidebar-foreground hover:bg-sidebar-accent focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
            data-testid="workspace-menu-trigger"
            type="button"
          >
            <span className="truncate">{name}</span>
            <ChevronDown aria-hidden="true" className="size-3 shrink-0" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem
            onSelect={onBrowseChannels}
            data-testid="workspace-browse-channels"
          >
            <Hash aria-hidden="true" className="size-4" /> Browse channels
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={onSelectWiki}
            data-testid="workspace-company-wiki"
          >
            <BookOpen aria-hidden="true" className="size-4" /> Company Wiki
          </DropdownMenuItem>
          <FeatureGate feature="pulse">
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onSelect={onSelectPulse}
              data-testid="open-pulse-view"
            >
              <Activity aria-hidden="true" className="size-4" /> Pulse
            </DropdownMenuItem>
          </FeatureGate>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
