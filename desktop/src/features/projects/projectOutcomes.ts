import { getNeedsYouForChannel } from "@/features/agents/needsYouStore";

export type ProjectOutcomeCounts = {
  needsYou: number;
  ready: number;
  inFlight: number;
  shipped30d: number;
};

export type ProjectOutcomeCard<TProject = ProjectLike> = {
  project: TProject;
  counts: ProjectOutcomeCounts;
  quiet: boolean;
};

type ProjectLike = {
  id: string;
  dtag: string;
  name: string;
  projectChannelId?: string | null;
  repositoryAddresses?: readonly string[];
  repositories?: readonly { channelId?: string | null }[];
};

type ActiveTurnLike = { channelId: string; conversationId?: string | null };
type OutcomeLike = {
  conversationId: string;
  channelId: string;
  outcome: "completed" | "error";
  endedAt: number;
};

type PullRequestLike = {
  projectId?: string | null;
  projectAddress?: string | null;
  repoAddress?: string | null;
  status: string;
  title?: string;
  author: string;
  updatedAt?: number;
  mergedAt?: number | null;
};

function projectChannels(project: ProjectLike): Set<string> {
  return new Set(
    [
      project.projectChannelId,
      ...(project.repositories ?? []).map((repository) => repository.channelId),
    ].filter((channelId): channelId is string => Boolean(channelId)),
  );
}

function pullRequestBelongsToProject(
  pullRequest: PullRequestLike,
  project: ProjectLike,
): boolean {
  if (
    pullRequest.projectId === project.id ||
    pullRequest.projectAddress === project.id
  ) {
    return true;
  }
  if (!pullRequest.repoAddress) return false;
  const projectRepositoryAddresses = new Set(
    (project.repositoryAddresses ?? []).map((address) => address.toLowerCase()),
  );
  return projectRepositoryAddresses.has(pullRequest.repoAddress.toLowerCase());
}

/** Derive the outcome-first card model without owning any relay/store state. */
export function deriveProjectOutcomeCards<TProject extends ProjectLike>(
  projects: readonly TProject[],
  activeTurns: readonly ActiveTurnLike[],
  outcomes: readonly OutcomeLike[],
  pullRequests: readonly PullRequestLike[],
  now: number,
): Array<ProjectOutcomeCard<TProject>> {
  const shippedSince = now - 30 * 24 * 60 * 60;
  const cards = projects.map((project) => {
    const channels = projectChannels(project);
    const inFlight = activeTurns.filter((turn) =>
      channels.has(turn.channelId),
    ).length;
    const ready = outcomes.filter(
      (outcome) =>
        outcome.outcome === "completed" && channels.has(outcome.channelId),
    ).length;
    const shipped30d = pullRequests.filter((pullRequest) => {
      const mergedAt = pullRequest.mergedAt ?? pullRequest.updatedAt;
      return (
        pullRequest.status === "Merged" &&
        mergedAt !== undefined &&
        mergedAt >= shippedSince &&
        pullRequestBelongsToProject(pullRequest, project)
      );
    }).length;
    const needsYou = [...channels].reduce(
      (count, channelId) => count + getNeedsYouForChannel(channelId).length,
      0,
    );
    const counts = { needsYou, ready, inFlight, shipped30d };
    return {
      project,
      counts,
      quiet: needsYou + ready + inFlight + shipped30d === 0,
    };
  });

  return cards.sort(
    (left, right) =>
      right.counts.needsYou - left.counts.needsYou ||
      right.counts.ready - left.counts.ready ||
      right.counts.inFlight - left.counts.inFlight ||
      left.project.name.localeCompare(right.project.name),
  );
}

export type ProjectShipLogEntry = {
  id: string;
  title: string;
  author: string;
  mergedAt: number;
};

export function projectShipLog(
  pullRequests: readonly (PullRequestLike & { id: string })[],
): ProjectShipLogEntry[] {
  return pullRequests
    .filter((pullRequest) => pullRequest.status === "Merged")
    .map((pullRequest) => ({
      id: pullRequest.id,
      title: pullRequest.title || "Untitled pull request",
      author: pullRequest.author,
      mergedAt: pullRequest.mergedAt ?? pullRequest.updatedAt ?? 0,
    }))
    .sort((left, right) => right.mergedAt - left.mergedAt);
}

export function partitionProjectCrew(
  pubkeys: readonly string[],
  upstreamPubkeys: ReadonlySet<string>,
): { crew: string[]; upstream: string[] } {
  const crew: string[] = [];
  const upstream: string[] = [];
  const seen = new Set<string>();
  for (const pubkey of pubkeys) {
    const key = pubkey.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    (upstreamPubkeys.has(key) ? upstream : crew).push(pubkey);
  }
  return { crew, upstream };
}
