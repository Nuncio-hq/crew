import { BookOpen, ChevronRight, Folder, Hash } from "lucide-react";
import type * as React from "react";

import type { Project } from "@/features/projects/projectModels";
import { listProjectBoundChannels } from "@/features/projects/lib/projectRelatedChannels";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";

/** Read-only Project projection. All user mutations remain with their operation controllers. */
export function CrewProjectOverview({
  project,
  channels,
  channelAction,
  channelStatus,
  workspaceAction,
  workspaceManagement,
  onOpenChannel,
  onOpenWiki,
  onOpenProjects,
}: {
  project: Project;
  channels: readonly Channel[];
  channelAction: React.ReactNode;
  channelStatus?: React.ReactNode;
  workspaceAction: React.ReactNode;
  workspaceManagement: (repositoryAddress: string) => React.ReactNode;
  onOpenChannel: (channelId: string) => void;
  onOpenWiki: () => void;
  onOpenProjects: () => void;
}) {
  const bound = listProjectBoundChannels(project);
  const channelById = new Map(channels.map((channel) => [channel.id, channel]));
  return (
    <section
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
      aria-label={`${project.name} project`}
      data-testid="project-overview"
    >
      <header className="flex min-w-0 items-center gap-2 border-b px-5 py-3 text-sm">
        <Folder
          aria-hidden="true"
          className="size-4 shrink-0 text-muted-foreground"
        />
        <Button onClick={onOpenProjects} variant="ghost" size="sm">
          Projects
        </Button>
        <ChevronRight aria-hidden="true" className="size-3 shrink-0" />
        <span className="truncate">{project.name}</span>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        <div className="mx-auto w-full max-w-4xl space-y-10 px-5 py-8 sm:px-8">
          <div className="space-y-3">
            <Folder
              aria-hidden="true"
              className="size-8 text-muted-foreground"
            />
            <p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">
              Project
            </p>
            <h1 className="break-words text-2xl font-semibold">
              {project.name}
            </h1>
            {project.description ? (
              <p className="max-w-2xl whitespace-pre-wrap break-words text-sm text-muted-foreground">
                {project.description}
              </p>
            ) : null}
            <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm text-muted-foreground">
              <Button onClick={onOpenWiki} variant="ghost" size="sm">
                <BookOpen aria-hidden="true" className="size-4" />
                Open Wiki
              </Button>
              <span>{bound.length} channels</span>
              <span>
                {project.repositories.length}{" "}
                {project.repositories.length === 1 ? "workspace" : "workspaces"}
              </span>
            </div>
          </div>
          <section
            aria-labelledby="project-channels-heading"
            className="space-y-4"
          >
            <header className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2
                  id="project-channels-heading"
                  className="text-base font-semibold"
                >
                  Channels
                </h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  Where conversations turn into work.
                </p>
              </div>
              {channelAction}
            </header>
            {channelStatus}
            <div className="divide-y rounded-xl border">
              {bound.map((binding) => {
                const channel = channelById.get(binding.channelId);
                return (
                  <button
                    key={binding.channelId}
                    type="button"
                    onClick={() => onOpenChannel(binding.channelId)}
                    className="flex w-full min-w-0 items-center gap-3 rounded-lg px-4 py-4 text-left hover:bg-muted/60 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
                    data-testid={`project-overview-channel-${binding.channelId}`}
                  >
                    <Hash
                      aria-hidden="true"
                      className="size-5 shrink-0 text-muted-foreground"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-2">
                        <strong className="break-all text-sm font-medium">
                          {channel?.name ?? binding.channelId}
                        </strong>
                        {binding.role === "home" ? (
                          <small className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                            Main channel
                          </small>
                        ) : null}
                      </span>
                      <span className="mt-1 block text-xs text-muted-foreground">
                        {channel?.description ||
                          (channel
                            ? "Open conversation"
                            : "Channel details unavailable")}
                      </span>
                    </span>
                    <ChevronRight
                      aria-hidden="true"
                      className="size-4 shrink-0 text-muted-foreground"
                    />
                  </button>
                );
              })}
              {bound.length === 0 ? (
                <p className="p-4 text-sm text-muted-foreground">
                  No channels linked yet.
                </p>
              ) : null}
            </div>
          </section>
          <section
            aria-labelledby="project-workspace-heading"
            className="space-y-4"
          >
            <header className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h2
                  id="project-workspace-heading"
                  className="text-base font-semibold"
                >
                  Workspace
                </h2>
                <p className="mt-1 text-sm text-muted-foreground">
                  Folders and repositories connected to this project.
                </p>
              </div>
              {workspaceAction}
            </header>
            <div className="space-y-3">
              {project.repositories.map((repository) => (
                <article
                  key={repository.repoAddress}
                  className="min-w-0 space-y-4 rounded-xl border p-4"
                  data-repository-address={repository.repoAddress}
                >
                  <div className="flex flex-wrap items-center gap-3">
                    <Folder
                      aria-hidden="true"
                      className="size-6 shrink-0 text-muted-foreground"
                    />
                    <div className="min-w-0 flex-1">
                      <h3 className="break-words text-sm font-medium">
                        {repository.name}
                      </h3>
                      <p className="text-xs text-muted-foreground">
                        {repository.workspaceMode === "folder"
                          ? "Folder workspace"
                          : "Git repository"}
                      </p>
                    </div>
                    {workspaceManagement(repository.repoAddress)}
                  </div>
                  {repository.localWorkspacePath ? (
                    <code className="block whitespace-pre-wrap break-all rounded-md bg-muted/50 px-3 py-2 text-xs">
                      {repository.localWorkspacePath}
                    </code>
                  ) : null}
                  <p className="text-xs text-muted-foreground">
                    {repository.localWorkspaceStatus === "invalid"
                      ? "Invalid workspace metadata"
                      : repository.localWorkspacePath
                        ? "Linked · access on this device not verified"
                        : "No folder linked"}
                  </p>
                </article>
              ))}
              {project.repositories.length === 0 ? (
                <div className="rounded-xl border border-dashed p-6 text-center">
                  <Folder
                    aria-hidden="true"
                    className="mx-auto mb-3 size-7 text-muted-foreground"
                  />
                  <h3 className="text-sm font-medium">
                    No workspace linked yet
                  </h3>
                  <p className="mt-2 text-sm text-muted-foreground">
                    Link a folder when you want agents to work with files.
                  </p>
                </div>
              ) : null}
            </div>
            {project.unavailableRepositoryAddresses?.length ? (
              <p role="status" className="text-sm text-muted-foreground">
                {project.unavailableRepositoryAddresses.length} repository{" "}
                {project.unavailableRepositoryAddresses.length === 1
                  ? "is"
                  : "are"}{" "}
                unavailable. Existing links are preserved.
              </p>
            ) : null}
            <p className="text-xs text-muted-foreground">
              Folders stay on their machine. Linking publishes the folder path
              as shared metadata; it does not synchronize files.
            </p>
          </section>
        </div>
      </div>
    </section>
  );
}
