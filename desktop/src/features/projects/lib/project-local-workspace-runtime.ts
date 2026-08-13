import { resolveProjectChannelAgentMessage } from "./project-channel-agent-context";
import type { WorkspaceBindingChoice } from "@/features/messages/lib/workspaceBindingSpec";
import {
  linkProjectLocalWorkspace,
  selectCurrentProjectAnnouncement,
  type ProjectRelayEvent,
} from "./project-local-workspace-relay";

import { relayClient } from "@/shared/api/relayClient";
import { createChannel } from "@/shared/api/tauriChannels";
import { getRelayWsUrl, signRelayEvent } from "@/shared/api/tauri";
import { getIdentity } from "@/shared/api/tauriIdentity";

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
  return linkProjectLocalWorkspace(input, {
    fetchEvents,
    signRelayEvent,
    publishEvent: async (
      event,
      timeoutMessage = "Timed out linking the Project workspace.",
      errorMessage = "Failed to link the Project workspace.",
    ) => {
      await relayClient.publishEvent(event, timeoutMessage, errorMessage);
    },
  });
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
