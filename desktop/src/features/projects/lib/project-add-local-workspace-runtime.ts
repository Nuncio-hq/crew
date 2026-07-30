import {
  createLocalWorkspaceProject,
  type LocalWorkspaceProjectInput,
} from "./project-add-local-workspace";
import { projectLocalWorkspaceFromEvent } from "./project-local-workspace";
import { publishProjectAnnouncementAndReadBack } from "./project-local-workspace-relay";
import {
  createProjectWorkspaceChannel,
  fetchCurrentProjectAnnouncement,
} from "./project-local-workspace-runtime";

import { eventToProject, type Project } from "@/features/projects/hooks";
import { relayClient } from "@/shared/api/relayClient";
import { getCachedRelayOrigin } from "@/shared/lib/mediaUrl";
import { signRelayEvent } from "@/shared/api/tauri";
import { getIdentity } from "@/shared/api/tauriIdentity";
import { KIND_REPO_ANNOUNCEMENT } from "@/shared/constants/kinds";

type RelayFilter = Parameters<typeof relayClient.fetchEvents>[0];

export async function createCurrentLocalWorkspaceProject(
  input: LocalWorkspaceProjectInput,
): Promise<Project> {
  const result = await createLocalWorkspaceProject(input, {
    createChannel: createProjectWorkspaceChannel,
    findProject: async (owner, dtag) => {
      const event = await fetchCurrentProjectAnnouncement(owner, dtag);
      if (!event) return null;
      const workspace = projectLocalWorkspaceFromEvent(event);
      return {
        channelId: workspace.channelId,
        dtag,
        localPath:
          workspace.localWorkspace.status === "linked"
            ? workspace.localWorkspace.path
            : null,
        owner,
        saved: eventToProject(event, getCachedRelayOrigin()),
      };
    },
    getOwnerPubkey: async () => (await getIdentity()).pubkey,
    publishAndReadBack: async ({ channelId, draft, localPath, owner }) => {
      const saved = await publishProjectAnnouncementAndReadBack(
        {
          event: {
            kind: KIND_REPO_ANNOUNCEMENT,
            content: draft.content,
            tags: draft.tags,
          },
          owner,
          dtag: draft.dtag,
          channelId,
          localPath,
        },
        {
          fetchEvents: (filter) =>
            relayClient.fetchEvents(filter as RelayFilter),
          signRelayEvent,
          publishEvent: async (event, timeoutMessage, errorMessage) => {
            await relayClient.publishEvent(
              event,
              timeoutMessage ?? "Timed out adding the Project.",
              errorMessage ?? "Failed to add the Project.",
            );
          },
        },
      );
      return eventToProject(saved, getCachedRelayOrigin());
    },
  });
  return result.saved;
}
