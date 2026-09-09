import { resolveProjectChannelAgentMessage } from "./project-channel-agent-context";
import type { WorkspaceBindingChoice } from "@/features/messages/lib/workspaceBindingSpec";
import {
  selectCurrentProjectAnnouncement,
  type ProjectRelayEvent,
} from "./project-local-workspace-relay";

import { relayClient } from "@/shared/api/relayClient";
import { createChannel } from "@/shared/api/tauriChannels";
import { getRelayWsUrl } from "@/shared/api/tauri";
import { getIdentity } from "@/shared/api/tauriIdentity";
import {
  linkProjectWorkspace,
  unlinkProjectWorkspace,
} from "@/shared/api/projectChannelLink";

type RelayFilter = Parameters<typeof relayClient.fetchEvents>[0];

function fetchEvents(filter: Record<string, unknown>) {
  return relayClient.fetchEvents(filter as RelayFilter);
}

export async function fetchCurrentProjectAnnouncement(
  owner: string,
  dtag: string,
): Promise<ProjectRelayEvent | null> {
  const events = await relayClient.fetchEvents({
    kinds: [30_617],
    authors: [owner],
    "#d": [dtag],
    limit: 50,
  });
  return selectCurrentProjectAnnouncement(events, owner, dtag);
}

export async function linkCurrentProjectWorkspace(input: {
  owner: string;
  currentPubkey: string;
  dtag: string;
  channelId: string;
  localPath: string;
}): Promise<ProjectRelayEvent> {
  if (input.owner.toLowerCase() !== input.currentPubkey.toLowerCase()) {
    throw new Error("Only the repository owner can link its workspace.");
  }
  const repositoryCoordinate = `30617:${input.owner.toLowerCase()}:${input.dtag}`;
  const result = await linkProjectWorkspace(
    repositoryCoordinate,
    input.channelId,
    input.localPath,
  );
  if (!result.value.reconciled || result.value.status !== "complete") {
    throw new Error(
      result.value.payload.last_error ??
        "The workspace link is still pending relay confirmation.",
    );
  }
  const saved = await fetchCurrentProjectAnnouncement(input.owner, input.dtag);
  if (!saved) {
    throw new Error(
      "The workspace was linked, but the repository could not be read.",
    );
  }
  return saved;
}

export async function unlinkCurrentProjectWorkspace(input: {
  owner: string;
  currentPubkey: string;
  dtag: string;
}): Promise<ProjectRelayEvent> {
  if (input.owner.toLowerCase() !== input.currentPubkey.toLowerCase()) {
    throw new Error(
      "Only the repository owner can unlink its local workspace.",
    );
  }
  const repositoryCoordinate = `30617:${input.owner.toLowerCase()}:${input.dtag}`;
  const result = await unlinkProjectWorkspace(repositoryCoordinate);
  if (!result.value.reconciled || result.value.status !== "complete") {
    throw new Error(
      result.value.payload.last_error ??
        "The workspace unlink is still pending relay confirmation.",
    );
  }
  const saved = await fetchCurrentProjectAnnouncement(input.owner, input.dtag);
  if (!saved) {
    throw new Error(
      "The workspace was unlinked, but the repository could not be read.",
    );
  }
  return saved;
}

export async function createProjectWorkspaceChannel(
  projectName: string,
): Promise<string> {
  const channel = await createChannel({
    name: `${projectName} project`,
    channelType: "stream",
    visibility: "open",
    description: `Project channel for ${projectName}`,
  });
  return channel.id;
}

export async function currentRelayWsUrl(): Promise<string> {
  return getRelayWsUrl();
}

export async function resolveCurrentProjectChannelAgentMessage(input: {
  channelId: string;
  content: string;
  explicitAgentPubkeys: string[];
  binding?: WorkspaceBindingChoice;
  defaultBranch?: string | null;
}): Promise<string> {
  const identity = await getIdentity();
  return resolveProjectChannelAgentMessage(
    { ...input, ownerPubkey: identity.pubkey },
    {
      fetchProjectAnnouncements: fetchEvents,
      fetchProjectDeletions: fetchEvents,
    },
  );
}
