import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { channelsQueryKey, useChannelsQuery } from "@/features/channels/hooks";
import type { Channel } from "@/shared/api/types";
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
  const queryClient = useQueryClient();
  useChannelsQuery();

  return React.useCallback(
    (channelId: string, selectedItems = items) => {
      // Read the cache at click time: joining a channel can finish before React
      // commits a new callback with the refreshed membership list.
      const channels = queryClient.getQueryData<Channel[]>(channelsQueryKey);
      if (!channels) {
        toast.error("Channels are still loading. Try again shortly.");
        return false;
      }
      if (!channels.some((channel) => channel.id === channelId)) {
        toast.error(
          "This channel is unavailable. Choose a channel you have joined.",
        );
        return false;
      }
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
      return true;
    },
    [goChannel, items, queryClient],
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
        return false;
      }
      const channelId = projectDiscussionChannelId({
        repositoryChannelId: repository?.channelId,
        projectChannelId: project?.projectChannelId,
        items,
        memberChannelIds: channelsQuery.data.map((channel) => channel.id),
      });
      if (!channelId) {
        toast.error("No accessible channel is linked to this repository.");
        return false;
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
      return discuss(channelId, context);
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
