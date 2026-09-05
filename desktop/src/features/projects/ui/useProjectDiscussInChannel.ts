import * as React from "react";
import { toast } from "sonner";
import { useChannelsQuery } from "@/features/channels/hooks";
import type { Project, Repository } from "@/features/projects/hooks";
import {
  repositoryShareLink,
  shareTabForWorkspaceTab,
} from "@/features/projects/lib/projectShareLinks";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import {
  loadDraftEntry,
  saveDraftEntry,
} from "@/features/messages/lib/useDrafts";
import {
  mergeSelectionDiscussDraft,
  projectDiscussionChannelId,
  selectionItemFromRepository,
  projectSelectionDiscussContent,
  type ProjectSelectionItem,
} from "@/features/projects/lib/projectSelection";

export function useProjectDiscussInChannel(items: ProjectSelectionItem[]) {
  const { goChannel } = useAppNavigation();

  return React.useCallback(
    (channelId: string, selectedItems = items) => {
      const now = new Date().toISOString();
      const existing = loadDraftEntry(channelId);
      const content = mergeSelectionDiscussDraft(
        existing?.content,
        projectSelectionDiscussContent(selectedItems),
      );
      saveDraftEntry(channelId, {
        channelId,
        content,
        createdAt: existing?.createdAt ?? now,
        mentionRefs: existing?.mentionRefs ?? [],
        pendingImeta: existing?.pendingImeta ?? [],
        selectionEnd: content.length,
        selectionStart: content.length,
        spoileredAttachmentUrls: existing?.spoileredAttachmentUrls ?? [],
        status: "active",
        updatedAt: now,
      });
      void goChannel(channelId);
    },
    [goChannel, items],
  );
}

/** Prepare repository context in the existing channel composer. */
export function useProjectRepositoryDiscussion({
  repository,
  project,
  activeTab,
}: {
  repository: Repository | null | undefined;
  project: Project | null | undefined;
  activeTab: string;
}) {
  const discuss = useProjectDiscussInChannel([]);
  const channelsQuery = useChannelsQuery();
  return React.useCallback(
    (items: ProjectSelectionItem[] = []) => {
      if (channelsQuery.isPending || !channelsQuery.data) {
        toast.error(
          channelsQuery.isError
            ? "Couldn’t load your channels. Try again."
            : "Channels are still loading. Try again shortly.",
        );
        return;
      }
      const channelId = projectDiscussionChannelId({
        repositoryChannelId: repository?.channelId,
        projectChannelId: project?.projectChannelId,
        items,
        memberChannelIds: channelsQuery.data.map((channel) => channel.id),
      });
      if (!channelId) {
        toast.error("No accessible channel is linked to this repository.");
        return;
      }
      const context =
        items.length > 0 || !repository
          ? items
          : [
              selectionItemFromRepository({
                id: repository.id,
                owner: repository.owner,
                channelId,
                shareLink: repositoryShareLink(
                  repository,
                  shareTabForWorkspaceTab(activeTab),
                ),
                title: `${repository.name} / ${activeTab === "activity" ? "Commits" : activeTab}`,
              }),
            ];
      discuss(channelId, context);
    },
    [
      activeTab,
      channelsQuery.data,
      channelsQuery.isPending,
      channelsQuery.isError,
      discuss,
      project?.projectChannelId,
      repository,
    ],
  );
}
