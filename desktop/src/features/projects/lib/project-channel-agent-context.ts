import {
  DEFAULT_WORKSPACE_BINDING,
  workspaceBindingQuerySuffix,
  type WorkspaceBindingChoice,
} from "@/features/messages/lib/workspaceBindingSpec";
import {
  projectLocalWorkspaceFromEvent,
  type LocalWorkspaceState,
} from "./project-local-workspace";
import type { ProjectRelayEvent } from "./project-local-workspace-relay";

type ProjectContextRecord = {
  id: string;
  repoAddress: string;
  projectChannelId: string | null;
  localWorkspace: LocalWorkspaceState;
};

export type ProjectChannelContext =
  | { status: "none" }
  | {
      status: "invalid";
      reason: "duplicate-project-channel-binding" | "invalid-local-workspace";
    }
  | { status: "ready"; repoAddress: string; localPath: string };

type RelayFilter = Record<string, unknown>;
type ResolverDependencies = {
  fetchProjectAnnouncements: (
    filter: RelayFilter,
  ) => Promise<ProjectRelayEvent[]>;
  fetchProjectDeletions: (filter: RelayFilter) => Promise<ProjectRelayEvent[]>;
};

function dtagOf(event: ProjectRelayEvent): string | null {
  const tags = event.tags.filter((tag) => tag[0] === "d" && tag[1]);
  return tags.length === 1 ? tags[0][1] : null;
}

function currentAnnouncements(events: ProjectRelayEvent[], owner: string) {
  const current = new Map<string, ProjectRelayEvent>();
  for (const event of events) {
    const dtag = dtagOf(event);
    if (
      event.kind !== 30_617 ||
      event.pubkey.toLowerCase() !== owner.toLowerCase() ||
      !dtag
    ) {
      continue;
    }
    const previous = current.get(dtag);
    if (
      !previous ||
      event.created_at > previous.created_at ||
      (event.created_at === previous.created_at && event.id < previous.id)
    ) {
      current.set(dtag, event);
    }
  }
  return [...current.entries()];
}

function isDeleted(
  event: ProjectRelayEvent,
  coordinate: string,
  deletions: ProjectRelayEvent[],
): boolean {
  return deletions.some(
    (deletion) =>
      deletion.kind === 5 &&
      deletion.pubkey.toLowerCase() === event.pubkey.toLowerCase() &&
      deletion.created_at >= event.created_at &&
      deletion.tags.some((tag) => tag[0] === "a" && tag[1] === coordinate),
  );
}

function recordsFromEvents(
  events: ProjectRelayEvent[],
  deletions: ProjectRelayEvent[],
  owner: string,
): ProjectContextRecord[] {
  return currentAnnouncements(events, owner).flatMap(([dtag, event]) => {
    const repoAddress = `30617:${event.pubkey}:${dtag}`;
    if (isDeleted(event, repoAddress, deletions)) return [];
    const parsed = projectLocalWorkspaceFromEvent(event);
    return [
      {
        id: `${event.pubkey}:${dtag}`,
        repoAddress,
        projectChannelId: parsed.channelId,
        localWorkspace: parsed.localWorkspace,
      },
    ];
  });
}

export function projectContextForChannel(
  channelId: string,
  projects: ProjectContextRecord[],
): ProjectChannelContext {
  const bindings = projects.filter(
    (project) => project.projectChannelId === channelId,
  );
  if (bindings.length === 0) return { status: "none" };
  if (bindings.length !== 1) {
    return { status: "invalid", reason: "duplicate-project-channel-binding" };
  }
  const match = bindings[0];
  if (match.localWorkspace.status === "invalid") {
    return { status: "invalid", reason: "invalid-local-workspace" };
  }
  if (match.localWorkspace.status === "unlinked") return { status: "none" };
  return {
    status: "ready",
    repoAddress: match.repoAddress,
    localPath: match.localWorkspace.path,
  };
}

export function appendProjectChannelAgentContext(
  content: string,
  context: ProjectChannelContext,
  binding: WorkspaceBindingChoice = DEFAULT_WORKSPACE_BINDING,
  defaultBranch: string | null = null,
): string {
  if (context.status === "none") return content;
  if (context.status === "invalid") {
    throw new Error(
      context.reason === "invalid-local-workspace"
        ? "Project has invalid local workspace metadata."
        : "Multiple Projects are bound to this channel.",
    );
  }
  const title = [
    `Project ${context.repoAddress}.`,
    `Source workspace ${context.localPath}.`,
    "The harness provisions one isolated worktree per thread.",
  ]
    .join(" ")
    .replaceAll("\\", "\\\\")
    .replaceAll('"', '\\"');
  const label = `buzz-project-context-${globalThis.crypto.randomUUID()}`;
  const workspaceUrl = [
    "buzz://project-workspace",
    `?repo=${encodeURIComponent(context.repoAddress)}`,
    `&path=${encodeURIComponent(context.localPath)}`,
    workspaceBindingQuerySuffix(binding, defaultBranch),
  ].join("");
  return `[${label}]: <${workspaceUrl}> "${title}"\n\n${content}`;
}

export async function resolveProjectChannelAgentMessage(
  input: {
    channelId: string;
    content: string;
    explicitAgentPubkeys: string[];
    ownerPubkey: string;
    binding?: WorkspaceBindingChoice;
    defaultBranch?: string | null;
  },
  dependencies: ResolverDependencies,
): Promise<string> {
  if (input.explicitAgentPubkeys.length === 0) return input.content;
  const filter = { authors: [input.ownerPubkey], limit: 2_000 };
  const [events, deletions] = await Promise.all([
    dependencies.fetchProjectAnnouncements({
      kinds: [30_617],
      ...filter,
    }),
    dependencies.fetchProjectDeletions({ kinds: [5], ...filter }),
  ]);
  const projects = recordsFromEvents(events, deletions, input.ownerPubkey);
  return appendProjectChannelAgentContext(
    input.content,
    projectContextForChannel(input.channelId, projects),
    input.binding,
    input.defaultBranch ?? null,
  );
}
