import {
  linkProjectWorkspaceTags,
  projectLocalWorkspaceFromEvent,
} from "./project-local-workspace";

const PROJECT_KIND = 30_617;

/** NIP-33 replacement must beat the live head, but stay inside the relay
 * ±15m ingest window (`MAX_TIMESTAMP_DRIFT_SECS`). `head + 1` alone rejects
 * any announcement older than 15 minutes. */
export function nextProjectAnnouncementCreatedAt(
  currentCreatedAt: number,
  now: number = Math.floor(Date.now() / 1_000),
): number {
  return Math.max(now, currentCreatedAt + 1);
}

export type ProjectRelayEvent = {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
  sig: string;
};

type ProjectEventInput = {
  kind: number;
  content: string;
  createdAt?: number;
  tags: string[][];
};

type RelayDependencies = {
  fetchEvents: (
    filter: Record<string, unknown>,
  ) => Promise<ProjectRelayEvent[]>;
  signRelayEvent: (event: ProjectEventInput) => Promise<ProjectRelayEvent>;
  publishEvent: (
    event: ProjectRelayEvent,
    timeoutMessage?: string,
    errorMessage?: string,
  ) => Promise<void>;
};

function tagValues(event: ProjectRelayEvent, name: string): string[] {
  return event.tags
    .filter((tag) => tag[0] === name && typeof tag[1] === "string")
    .map((tag) => tag[1]);
}

function isExpectedProject(
  event: ProjectRelayEvent,
  owner: string,
  dtag: string,
): boolean {
  return (
    event.kind === PROJECT_KIND &&
    event.pubkey.toLowerCase() === owner.toLowerCase() &&
    tagValues(event, "d").length === 1 &&
    tagValues(event, "d")[0] === dtag
  );
}

export function selectCurrentProjectAnnouncement(
  events: ProjectRelayEvent[],
  owner: string,
  dtag: string,
): ProjectRelayEvent | null {
  return (
    events
      .filter((event) => isExpectedProject(event, owner, dtag))
      .sort(
        (left, right) =>
          right.created_at - left.created_at || left.id.localeCompare(right.id),
      )[0] ?? null
  );
}

function validateReadBack(
  event: ProjectRelayEvent | undefined,
  expected: {
    id: string;
    owner: string;
    dtag: string;
    channelId: string;
    localPath: string;
  },
): ProjectRelayEvent {
  const workspace = event
    ? projectLocalWorkspaceFromEvent(event)
    : { channelId: null, localWorkspace: { status: "unlinked" as const } };
  if (
    !event ||
    event.id !== expected.id ||
    !isExpectedProject(event, expected.owner, expected.dtag) ||
    workspace.channelId !== expected.channelId ||
    workspace.localWorkspace.status !== "linked" ||
    workspace.localWorkspace.path !== expected.localPath
  ) {
    throw new Error("Project save failed relay read-back validation.");
  }
  return event;
}

export async function publishProjectAnnouncementAndReadBack(
  input: {
    event: ProjectEventInput;
    owner: string;
    dtag: string;
    channelId: string;
    localPath: string;
  },
  dependencies: RelayDependencies,
): Promise<ProjectRelayEvent> {
  const signed = await dependencies.signRelayEvent(input.event);
  if (signed.pubkey.toLowerCase() !== input.owner.toLowerCase()) {
    throw new Error(
      "Signed Project owner does not match the selected Project.",
    );
  }
  validateReadBack(signed, {
    id: signed.id,
    owner: input.owner,
    dtag: input.dtag,
    channelId: input.channelId,
    localPath: input.localPath,
  });
  await dependencies.publishEvent(
    signed,
    "Timed out linking the Project workspace.",
    "Failed to link the Project workspace.",
  );
  const readBack = await dependencies.fetchEvents({
    ids: [signed.id],
    kinds: [PROJECT_KIND],
    authors: [signed.pubkey],
    "#d": [input.dtag],
    limit: 1,
  });
  return validateReadBack(readBack[0], {
    id: signed.id,
    owner: input.owner,
    dtag: input.dtag,
    channelId: input.channelId,
    localPath: input.localPath,
  });
}

export async function linkProjectLocalWorkspace(
  input: {
    owner: string;
    currentPubkey: string;
    dtag: string;
    channelId: string;
    localPath: string;
  },
  dependencies: RelayDependencies,
): Promise<ProjectRelayEvent> {
  if (input.owner.toLowerCase() !== input.currentPubkey.toLowerCase()) {
    throw new Error("Only the owner can link their own Project workspace.");
  }
  const events = await dependencies.fetchEvents({
    kinds: [PROJECT_KIND],
    authors: [input.owner],
    "#d": [input.dtag],
    limit: 50,
  });
  const current = selectCurrentProjectAnnouncement(
    events,
    input.owner,
    input.dtag,
  );
  if (!current) throw new Error("Project announcement was not found on relay.");

  const durableTags = current.tags.filter((tag) => tag[0] !== "auth");
  const tags = linkProjectWorkspaceTags(durableTags, input);
  return publishProjectAnnouncementAndReadBack(
    {
      event: {
        kind: PROJECT_KIND,
        content: current.content,
        createdAt: nextProjectAnnouncementCreatedAt(current.created_at),
        tags,
      },
      owner: input.owner,
      dtag: input.dtag,
      channelId: input.channelId,
      localPath: input.localPath,
    },
    dependencies,
  );
}
