export type ProjectAnnouncementUiStatus =
  | "loading"
  | "error"
  | "missing"
  | "ready";

export type RetryProjectChannel = {
  projectId: string;
  channelId: string;
};

export function projectWorkspaceUiReadiness(input: {
  announcementStatus: ProjectAnnouncementUiStatus;
  relayUrl: string | null | undefined;
}): { canChooseFolder: boolean; canPublish: boolean } {
  const canChooseFolder = input.announcementStatus === "ready";
  return {
    canChooseFolder,
    canPublish:
      canChooseFolder &&
      typeof input.relayUrl === "string" &&
      input.relayUrl.trim().length > 0,
  };
}

export function reusableProjectWorkspaceChannel(
  projectId: string,
  canonicalChannelId: string | null,
  retryChannel: RetryProjectChannel | null,
): string | null {
  if (canonicalChannelId) return canonicalChannelId;
  return retryChannel?.projectId === projectId ? retryChannel.channelId : null;
}
