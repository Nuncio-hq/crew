import {
  ChevronDown,
  Circle,
  FolderGit2,
  Rocket,
  Users,
  Zap,
} from "lucide-react";
import * as React from "react";

import {
  getActiveTurnsGeneration,
  subscribeActiveAgentTurns,
  walkActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import type { Project, ProjectPullRequest } from "@/features/projects/hooks";
import {
  partitionProjectCrew,
  projectShipLog,
} from "@/features/projects/projectOutcomes";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { resolveUserLabel } from "@/features/profile/lib/identity";
import { cn } from "@/shared/lib/cn";
import { ProjectOutcomeThreadPanel } from "./ProjectOutcomeThreadPanel";

export function ProjectOutcomeDetail({
  children,
  openPlumbing = false,
  project,
  profiles,
  pullRequests,
}: {
  children: React.ReactNode;
  openPlumbing?: boolean;
  project: Project;
  profiles?: UserProfileLookup;
  pullRequests: ProjectPullRequest[];
}) {
  const activeTurnsGeneration = React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getActiveTurnsGeneration,
    getActiveTurnsGeneration,
  );
  void activeTurnsGeneration;
  const activeTurns = new Map<
    string,
    { channelId: string; conversationId: string }
  >();
  walkActiveAgentTurns((_agentKey, turn) => {
    activeTurns.set(`${turn.channelId}:${turn.conversationId}`, {
      channelId: turn.channelId,
      conversationId: turn.conversationId,
    });
  });
  const [openThread, setOpenThread] = React.useState<{
    channelId: string;
    conversationId: string;
  } | null>(null);
  const [plumbingOpen, setPlumbingOpen] = React.useState(openPlumbing);
  React.useEffect(() => {
    if (openPlumbing) setPlumbingOpen(true);
  }, [openPlumbing]);
  const projectChannelIds = new Set(
    [
      project.projectChannelId,
      ...project.repositories.map((repository) => repository.channelId),
    ].filter((channelId): channelId is string => Boolean(channelId)),
  );
  const inFlight = [...activeTurns.values()].filter(({ channelId }) =>
    projectChannelIds.has(channelId),
  );
  const shipLog = projectShipLog(
    pullRequests.map((pullRequest) => ({
      id: pullRequest.id,
      title: pullRequest.title,
      author: pullRequest.author,
      status: pullRequest.status,
      updatedAt: pullRequest.updatedAt,
      mergedAt:
        pullRequest.status === "Merged"
          ? (pullRequest.statusCreatedAt ?? pullRequest.updatedAt)
          : null,
    })),
  );
  const contributors = [
    project.owner,
    ...project.repositories.flatMap((repository) => repository.contributors),
    ...pullRequests.flatMap((pullRequest) => [
      pullRequest.author,
      ...pullRequest.recipients,
    ]),
  ];
  const upstreamPubkeys = new Set(
    project.repositories
      .flatMap((repository) => repository.contributors)
      .filter((pubkey) => pubkey.toLowerCase() !== project.owner.toLowerCase())
      .map((pubkey) => pubkey.toLowerCase()),
  );
  const crew = partitionProjectCrew(contributors, upstreamPubkeys);

  return (
    <div className="flex min-w-0 gap-4" data-testid="project-outcome-page">
      <div className="min-w-0 flex-1 space-y-4">
        <section className="rounded-xl border border-border/60 bg-muted/10 p-4">
          <div className="flex items-center gap-3">
            <span className="flex h-10 w-10 items-center justify-center rounded-xl border border-border/60 bg-muted/40">
              <FolderGit2 className="h-5 w-5 text-muted-foreground" />
            </span>
            <div>
              <p className="text-2xs uppercase tracking-[0.18em] text-muted-foreground">
                Project outcome
              </p>
              <h1 className="text-lg font-semibold text-foreground">
                {project.name}
              </h1>
            </div>
          </div>
        </section>

        <section
          className="rounded-xl border border-border/60"
          data-testid="project-ship-log"
        >
          <div className="flex items-center gap-2 border-b border-border/50 px-4 py-3">
            <Rocket className="h-4 w-4 text-muted-foreground" />
            <h2 className="text-sm font-semibold text-foreground">Ship log</h2>
          </div>
          {shipLog.length === 0 ? (
            <p className="px-4 py-4 text-sm text-muted-foreground">
              Nothing has shipped yet.
            </p>
          ) : (
            <ul className="divide-y divide-border/50">
              {shipLog.map((entry) => (
                <li
                  className="flex items-center gap-3 px-4 py-3"
                  key={entry.id}
                >
                  <span className="h-2 w-2 rounded-full bg-success" />
                  <span className="min-w-0 flex-1 truncate text-sm text-foreground">
                    {entry.title}
                  </span>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {resolveUserLabel({ pubkey: entry.author, profiles })}
                  </span>
                  <time
                    className="shrink-0 text-xs text-muted-foreground"
                    dateTime={new Date(entry.mergedAt * 1_000).toISOString()}
                  >
                    {new Date(entry.mergedAt * 1_000).toLocaleDateString()}
                  </time>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section
          className="rounded-xl border border-border/60"
          data-testid="project-in-flight"
        >
          <div className="flex items-center gap-2 border-b border-border/50 px-4 py-3">
            <Circle className="h-4 w-4 text-attention" />
            <h2 className="text-sm font-semibold text-foreground">In flight</h2>
          </div>
          {inFlight.length === 0 ? (
            <p className="px-4 py-4 text-sm text-muted-foreground">
              No active threads.
            </p>
          ) : (
            <ul className="divide-y divide-border/50">
              {inFlight.map((turn) => (
                <li key={turn.conversationId}>
                  <button
                    className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-muted/20"
                    data-testid={`project-in-flight-${turn.conversationId}`}
                    onClick={() => setOpenThread(turn)}
                    type="button"
                  >
                    <Zap className="h-4 w-4 text-attention" />
                    <span className="min-w-0 flex-1 truncate text-sm text-foreground">
                      Active work in {turn.channelId}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      Open in panel
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section
          className="rounded-xl border border-border/60"
          data-testid="project-crew"
        >
          <div className="flex items-center gap-2 border-b border-border/50 px-4 py-3">
            <Users className="h-4 w-4 text-muted-foreground" />
            <h2 className="text-sm font-semibold text-foreground">Crew</h2>
          </div>
          <div className="space-y-2 p-4">
            <div className="flex flex-wrap gap-2">
              {crew.crew.map((pubkey) => (
                <span
                  className="rounded-full bg-muted px-2.5 py-1 text-xs text-foreground"
                  key={pubkey}
                >
                  {resolveUserLabel({ pubkey, profiles })}
                </span>
              ))}
            </div>
            {crew.upstream.length > 0 ? (
              <details>
                <summary className="cursor-pointer text-xs text-muted-foreground">
                  {crew.upstream.length} upstream contributors
                </summary>
                <div className="mt-2 flex flex-wrap gap-2">
                  {crew.upstream.map((pubkey) => (
                    <span
                      className="text-xs text-muted-foreground"
                      key={pubkey}
                    >
                      {resolveUserLabel({ pubkey, profiles })}
                    </span>
                  ))}
                </div>
              </details>
            ) : null}
          </div>
        </section>

        <details
          data-testid="project-plumbing"
          onToggle={(event) => setPlumbingOpen(event.currentTarget.open)}
          open={plumbingOpen}
        >
          <summary className="flex cursor-pointer list-none items-center gap-2 rounded-xl border border-border/60 px-4 py-3 text-sm font-semibold text-foreground">
            <ChevronDown className="h-4 w-4" />
            Plumbing
            <span className="ml-auto text-xs font-normal text-muted-foreground">
              Files · Commits · Issues · PRs · Contributors
            </span>
          </summary>
          <div className={cn("mt-3")}>{children}</div>
        </details>
      </div>
      {openThread ? (
        <ProjectOutcomeThreadPanel
          channelId={openThread.channelId}
          conversationId={openThread.conversationId}
          onClose={() => setOpenThread(null)}
          profiles={profiles}
        />
      ) : null}
    </div>
  );
}
