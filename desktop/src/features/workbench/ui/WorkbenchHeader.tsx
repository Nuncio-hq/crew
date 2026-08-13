import { Hash } from "lucide-react";

import {
  ProjectThreadWorkspacePanel,
  SessionAgingBannerSlot,
} from "../lib/workbenchSharedRenderers";
import type { ProjectThreadWorkspaceModel } from "@/features/messages/ui/useProjectThreadWorkspaceModel";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { Button } from "@/shared/ui/button";

export function WorkbenchHeader({
  channelId,
  channelName,
  conversationId,
  officeView,
  onOpenChannel,
  onToggleOffice,
  profiles,
  rootEventId,
  title,
  workspaceModel,
}: {
  channelId: string;
  channelName: string;
  conversationId: string | null;
  officeView: boolean;
  onOpenChannel: () => void;
  onToggleOffice: () => void;
  profiles?: UserProfileLookup;
  rootEventId: string;
  title: string;
  workspaceModel: ProjectThreadWorkspaceModel | null;
}) {
  return (
    <header
      className="shrink-0 border-b border-border/60 px-4 py-2"
      data-testid="workbench-header"
    >
      <div className="flex min-w-0 items-start gap-3">
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-base font-semibold text-foreground">
            {title}
          </h1>
          {workspaceModel ? (
            <div className="mt-2">
              <ProjectThreadWorkspacePanel
                channelId={channelId}
                isFocusMode
                model={workspaceModel}
                profiles={profiles}
              />
            </div>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            data-testid="workbench-office-toggle"
            onClick={onToggleOffice}
            size="sm"
            type="button"
            variant={officeView ? "secondary" : "ghost"}
          >
            {officeView ? "Workbench view" : "Office view"}
          </Button>
          <Button
            className="gap-1"
            data-testid="workbench-open-channel"
            onClick={onOpenChannel}
            size="sm"
            type="button"
            variant="outline"
          >
            <Hash className="h-3.5 w-3.5" />
            {channelName}
          </Button>
        </div>
      </div>
      <SessionAgingBannerSlot
        conversationIds={conversationId ? [conversationId] : []}
        profiles={profiles}
        rootEventId={rootEventId}
      />
    </header>
  );
}
