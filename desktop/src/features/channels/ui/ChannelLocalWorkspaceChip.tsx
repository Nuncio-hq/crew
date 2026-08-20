import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  exclusiveChannelLocalWorkspace,
  type ChannelLocalWorkspace,
} from "@/features/channels/lib/channelLocalWorkspace";
import { projectsQueryKey, useProjectsQuery } from "@/features/projects/hooks";
import { linkCurrentProjectWorkspace } from "@/features/projects/lib/project-local-workspace-runtime";
import { useIdentityQuery } from "@/shared/api/hooks";
import { chooseProjectWorkspaceFolder } from "@/shared/api/tauri-project-folder-dialog";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";

const GONE_FOLDER_COPY = "The Project folder is gone. Pick a workspace again.";
const LINKED_TOAST = "Project workspace linked. Send a new message to use it.";

export type ChannelLocalWorkspaceChipView = {
  fullPath: string;
  pathLabel: string;
  actionLabel: "Relink folder" | "Pick folder" | null;
  goneMessage: string | null;
};

export function truncateLocalWorkspacePath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 2) return path;
  return `…/${parts.slice(-2).join("/")}`;
}

export function buildChannelLocalWorkspaceChipView(input: {
  binding: ChannelLocalWorkspace | null;
  currentPubkey: string | null | undefined;
  pathMissing?: boolean;
}): ChannelLocalWorkspaceChipView | null {
  if (!input.binding) return null;
  const owner =
    Boolean(input.currentPubkey) &&
    normalizePubkey(input.currentPubkey ?? "") ===
      normalizePubkey(input.binding.owner);
  return {
    fullPath: input.binding.localPath,
    pathLabel: truncateLocalWorkspacePath(input.binding.localPath),
    actionLabel: owner
      ? input.pathMissing
        ? "Pick folder"
        : "Relink folder"
      : null,
    goneMessage: input.pathMissing ? GONE_FOLDER_COPY : null,
  };
}

export function ChannelLocalWorkspaceChip({
  channelId,
}: {
  channelId: string | null;
}) {
  const projectsQuery = useProjectsQuery();
  const identityQuery = useIdentityQuery();
  const queryClient = useQueryClient();
  const binding = exclusiveChannelLocalWorkspace(channelId, projectsQuery.data);
  const view = buildChannelLocalWorkspaceChipView({
    binding,
    currentPubkey: identityQuery.data?.pubkey,
  });

  const relink = React.useCallback(async () => {
    if (!binding || !channelId) return;
    const currentPubkey = identityQuery.data?.pubkey;
    if (!currentPubkey) {
      toast.error("Could not link workspace.");
      return;
    }
    try {
      const path = await chooseProjectWorkspaceFolder();
      if (!path) return;
      await linkCurrentProjectWorkspace({
        owner: binding.owner,
        currentPubkey,
        dtag: binding.dtag,
        channelId,
        localPath: path,
      });
      await queryClient.invalidateQueries({
        queryKey: ["crew-project-announcement"],
      });
      await queryClient.invalidateQueries({ queryKey: projectsQueryKey });
      toast.success(LINKED_TOAST);
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Could not link workspace.",
      );
    }
  }, [binding, channelId, identityQuery.data?.pubkey, queryClient]);

  if (!view) return null;

  return (
    <div
      className="flex min-w-0 max-w-64 items-center gap-1"
      data-testid="channel-local-workspace-chip"
    >
      <span
        className="min-w-0 truncate text-xs text-muted-foreground"
        title={view.fullPath}
      >
        {view.pathLabel}
      </span>
      {view.actionLabel ? (
        <Button
          className="shrink-0 text-xs"
          data-testid="channel-local-workspace-relink"
          onClick={() => void relink()}
          size="sm"
          type="button"
          variant="ghost"
        >
          {view.actionLabel}
        </Button>
      ) : null}
    </div>
  );
}
