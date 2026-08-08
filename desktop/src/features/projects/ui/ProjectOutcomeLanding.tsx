import { Circle, FolderGit2, PackageCheck, Rocket, Zap } from "lucide-react";
import * as React from "react";

import {
  getActiveTurnsGeneration,
  walkActiveAgentTurns,
  walkConversationOutcomes,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import { useNeedsYouForChannels } from "@/features/agents/needsYouStore";
import { getRecentOutcomeForConversation } from "@/features/agents/recentConversationOutcomes";
import type { Project, ProjectPullRequest } from "@/features/projects/hooks";
import {
  deriveProjectOutcomeCards,
  type ProjectOutcomeCard as OutcomeCard,
} from "@/features/projects/projectOutcomes";
import { cn } from "@/shared/lib/cn";
import { Card } from "@/shared/ui/card";

function useOutcomeLedger() {
  const generation = React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getActiveTurnsGeneration,
    getActiveTurnsGeneration,
  );
  return React.useMemo(() => {
    const outcomes: Array<{
      conversationId: string;
      channelId: string;
      outcome: "completed" | "error";
      endedAt: number;
    }> = [];
    walkConversationOutcomes((conversationId) => {
      const recent = getRecentOutcomeForConversation(conversationId);
      if (recent && recent.outcome !== "lost-contact") {
        outcomes.push({
          conversationId,
          channelId: recent.channelId,
          outcome: recent.outcome,
          endedAt: recent.endedAt,
        });
      }
    });
    return { generation, outcomes };
  }, [generation]);
}

function useActiveThreadTurns() {
  const generation = React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getActiveTurnsGeneration,
    getActiveTurnsGeneration,
  );
  void generation;
  const turns = new Map<
    string,
    { channelId: string; conversationId: string }
  >();
  walkActiveAgentTurns((_agentKey, turn) => {
    turns.set(`${turn.channelId}:${turn.conversationId}`, {
      channelId: turn.channelId,
      conversationId: turn.conversationId,
    });
  });
  return [...turns.values()];
}

function OutcomeMetric({
  icon: Icon,
  label,
  value,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  value: number;
}) {
  return (
    <span
      className="inline-flex items-center gap-1 text-2xs text-muted-foreground"
      title={label}
    >
      <Icon className="h-3 w-3" />
      <span className="font-semibold text-foreground">{value}</span>
    </span>
  );
}

function ProjectOutcomeCard({
  card,
  onOpen,
}: {
  card: OutcomeCard<Project>;
  onOpen: (project: Project) => void;
}) {
  const { project, counts } = card;
  return (
    <Card
      className={cn(
        "group relative overflow-hidden border-border/60 bg-transparent shadow-none transition-colors hover:bg-muted/20",
        card.quiet && "opacity-55",
      )}
      data-testid={`project-outcome-card-${project.dtag}`}
    >
      <button
        aria-label={`Open project ${project.name}`}
        className="absolute inset-0 z-0 cursor-pointer"
        onClick={() => onOpen(project)}
        type="button"
      />
      <div className="pointer-events-none relative z-10 space-y-4 p-4">
        <div className="flex items-center gap-3">
          <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-border/60 bg-muted/40">
            <FolderGit2 className="h-5 w-5 text-muted-foreground" />
          </span>
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold text-foreground">
              {project.name}
            </h2>
            <p className="text-xs text-muted-foreground">
              {project.repositoryAddresses.length === 0
                ? "Project"
                : `${project.repositoryAddresses.length} ${project.repositoryAddresses.length === 1 ? "repository" : "repositories"}`}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-border/50 pt-3">
          <OutcomeMetric icon={Zap} label="Needs you" value={counts.needsYou} />
          <OutcomeMetric
            icon={PackageCheck}
            label="Ready"
            value={counts.ready}
          />
          <OutcomeMetric
            icon={Circle}
            label="In flight"
            value={counts.inFlight}
          />
          <OutcomeMetric
            icon={Rocket}
            label="Shipped in the last 30 days"
            value={counts.shipped30d}
          />
        </div>
      </div>
    </Card>
  );
}

export function ProjectOutcomeLanding({
  onOpen,
  projects,
  pullRequests,
}: {
  onOpen: (project: Project) => void;
  projects: Project[];
  pullRequests: Array<{ project: Project; pullRequest: ProjectPullRequest }>;
}) {
  const { outcomes } = useOutcomeLedger();
  const projectChannelIds = React.useMemo(
    () =>
      projects
        .flatMap((project) => [
          project.projectChannelId,
          ...project.repositories.map((repository) => repository.channelId),
        ])
        .filter((channelId): channelId is string => Boolean(channelId)),
    [projects],
  );
  useNeedsYouForChannels(projectChannelIds);
  const activeTurns = useActiveThreadTurns();
  const cards = deriveProjectOutcomeCards(
    projects,
    activeTurns,
    outcomes,
    pullRequests.map(({ project, pullRequest }) => ({
      projectId: project.id,
      repoAddress: pullRequest.repoAddress,
      status: pullRequest.status,
      title: pullRequest.title,
      author: pullRequest.author,
      updatedAt: pullRequest.updatedAt,
      mergedAt:
        pullRequest.status === "Merged"
          ? (pullRequest.statusCreatedAt ?? pullRequest.updatedAt)
          : null,
    })),
    Math.floor(Date.now() / 1_000),
  );

  return (
    <section className="space-y-3" data-testid="projects-outcome-landing">
      <div className="flex items-end justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold text-foreground">
            What needs attention?
          </h1>
          <p className="text-sm text-muted-foreground">
            Projects, organized by outcome.
          </p>
        </div>
      </div>
      {cards.length > 0 ? (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {cards.map((card) => (
            <ProjectOutcomeCard
              card={card}
              key={card.project.id}
              onOpen={onOpen}
            />
          ))}
        </div>
      ) : (
        <div className="rounded-xl border border-dashed border-border/60 p-8 text-center text-sm text-muted-foreground">
          No projects yet.
        </div>
      )}
    </section>
  );
}
