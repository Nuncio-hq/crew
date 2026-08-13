export const PROJECT_LOCAL_LOCATION_TAG = "buzz-location";
export const PROJECT_CHANNEL_TAG = "buzz-channel";
export const CREW_WORKSPACE_MODE_TAG = "crew-workspace-mode";

export type CrewWorkspaceMode = "git" | "folder";

export type LocalWorkspaceState =
  | { status: "unlinked" }
  | { status: "linked"; path: string }
  | {
      status: "invalid";
      reason: "invalid-local-path" | "duplicate-local-paths";
    };

type ProjectEventLike = {
  tags: string[][];
};

export function validateLocalWorkspacePath(path: string): string {
  if (
    typeof path !== "string" ||
    path.length === 0 ||
    !path.startsWith("/") ||
    /[\0\r\n]/u.test(path)
  ) {
    throw new Error("Choose an absolute local folder path.");
  }
  return path;
}

export function readCanonicalProjectChannel(
  tags: string[][],
):
  | { status: "absent" }
  | { status: "ready"; channelId: string }
  | { status: "invalid" } {
  const channelTags = tags.filter((tag) => tag[0] === PROJECT_CHANNEL_TAG);
  if (channelTags.length === 0) return { status: "absent" };
  if (
    channelTags.length !== 1 ||
    channelTags[0].length !== 2 ||
    !channelTags[0][1]
  ) {
    return { status: "invalid" };
  }
  return { status: "ready", channelId: channelTags[0][1] };
}

function canonicalChannelFromTags(tags: string[][]): string | null {
  const channel = readCanonicalProjectChannel(tags);
  return channel.status === "ready" ? channel.channelId : null;
}

export function projectLocalWorkspaceFromEvent(event: ProjectEventLike): {
  channelId: string | null;
  localWorkspace: LocalWorkspaceState;
} {
  const localTags = event.tags.filter(
    (tag) => tag[0] === PROJECT_LOCAL_LOCATION_TAG && tag[1] === "local",
  );
  if (localTags.length === 0) {
    return {
      channelId: canonicalChannelFromTags(event.tags),
      localWorkspace: { status: "unlinked" },
    };
  }
  if (localTags.length !== 1) {
    return {
      channelId: canonicalChannelFromTags(event.tags),
      localWorkspace: {
        status: "invalid",
        reason: "duplicate-local-paths",
      },
    };
  }

  try {
    const path = validateLocalWorkspacePath(localTags[0][2] ?? "");
    return {
      channelId: canonicalChannelFromTags(event.tags),
      localWorkspace: { status: "linked", path },
    };
  } catch {
    return {
      channelId: canonicalChannelFromTags(event.tags),
      localWorkspace: { status: "invalid", reason: "invalid-local-path" },
    };
  }
}

export function replaceLocalWorkspaceTag(
  tags: string[][],
  path: string,
): string[][] {
  const validatedPath = validateLocalWorkspacePath(path);
  return [
    ...tags
      .filter(
        (tag) => !(tag[0] === PROJECT_LOCAL_LOCATION_TAG && tag[1] === "local"),
      )
      .map((tag) => [...tag]),
    [PROJECT_LOCAL_LOCATION_TAG, "local", validatedPath],
  ];
}

export function linkProjectWorkspaceTags(
  tags: string[][],
  input: { channelId: string; localPath: string },
): string[][] {
  const channelTags = tags.filter((tag) => tag[0] === PROJECT_CHANNEL_TAG);
  if (
    !input.channelId ||
    channelTags.length > 1 ||
    channelTags.some((tag) => tag.length !== 2 || !tag[1]) ||
    (channelTags.length === 1 && channelTags[0][1] !== input.channelId)
  ) {
    throw new Error("Project has an invalid canonical Project channel.");
  }

  const withChannel =
    channelTags.length === 1
      ? tags.map((tag) => [...tag])
      : [
          ...tags.map((tag) => [...tag]),
          [PROJECT_CHANNEL_TAG, input.channelId],
        ];
  return replaceLocalWorkspaceTag(withChannel, input.localPath);
}

export function localWorkspacePrivacyNotice(relayUrl: string): string {
  return [
    "The raw local path will be plaintext metadata in a signed Project event.",
    `It will be published to ${relayUrl}.`,
    "Anyone who can read that relay event can read the path.",
  ].join(" ");
}

export function readCrewWorkspaceMode(tags: string[][]): CrewWorkspaceMode {
  const tag = tags.find(
    (candidate) => candidate[0] === CREW_WORKSPACE_MODE_TAG,
  );
  return tag?.[1] === "folder" ? "folder" : "git";
}

export function withCrewWorkspaceMode(
  tags: string[][],
  mode: CrewWorkspaceMode,
): string[][] {
  const next = tags
    .filter((tag) => tag[0] !== CREW_WORKSPACE_MODE_TAG)
    .map((tag) => [...tag]);
  if (mode === "folder") {
    next.push([CREW_WORKSPACE_MODE_TAG, "folder"]);
  }
  return next;
}
