import { Check, ChevronDown, ExternalLink, GitBranch } from "lucide-react";

import type { Project, Repository } from "@/features/projects/hooks";
import type { useProjectRepoPresentation } from "@/features/projects/useProjectRepoHost";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import type { ProjectDetailWorkItemCrumb } from "./ProjectDetailChrome";
import { ProjectRepositoryManagement } from "./ProjectRepositoryManagement";

export function ProjectDetailRepositoryHeader({
  identityPubkey,
  onRepositoryChange,
  project,
  projects,
  repoRemote,
  repoSource,
  repository,
}: {
  identityPubkey: string | undefined;
  onRepositoryChange: (repositoryId: string) => void;
  project: Project;
  projects: Project[];
  repoRemote: ReturnType<typeof useProjectRepoPresentation>;
  repoSource: "local" | "remote";
  repository: Repository;
}) {
  return (
    <section className="space-y-3" data-testid="project-repository-header">
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0 flex-1 space-y-0.5">
          <div className="flex min-w-0 items-center gap-1.5">
            <h2 className="truncate text-xl font-semibold tracking-tight">
              {project.name}
            </h2>
            {repoRemote.webUrl &&
            (repoRemote.host.kind !== "external" || repoSource === "local") ? (
              <Button
                asChild
                aria-label="Open project web page"
                className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
                size="icon-xs"
                variant="ghost"
              >
                <a
                  href={repoRemote.webUrl}
                  rel="noopener noreferrer"
                  target="_blank"
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                </a>
              </Button>
            ) : null}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="hidden text-xs font-medium text-muted-foreground sm:inline">
            Repository
          </span>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                aria-label="Select repository"
                className="h-7 max-w-48 gap-1.5"
                data-testid="project-repository-picker"
                size="sm"
                variant="outline"
              >
                <GitBranch className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate">{repository.name}</span>
                <ChevronDown className="h-3.5 w-3.5 shrink-0" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>Repositories</DropdownMenuLabel>
              {project.repositories.map((candidate) => (
                <DropdownMenuItem
                  key={candidate.id}
                  data-testid={`project-repository-${candidate.dtag}`}
                  onSelect={() => onRepositoryChange(candidate.id)}
                >
                  <span className="min-w-0 flex-1 truncate">
                    {candidate.name}
                  </span>
                  {candidate.id === repository.id ? (
                    <Check className="h-4 w-4 shrink-0" />
                  ) : null}
                </DropdownMenuItem>
              ))}
              {(project.unavailableRepositoryAddresses ?? []).map((address) => (
                <DropdownMenuItem key={address} disabled>
                  <span className="min-w-0 flex-1 truncate">
                    {address.slice(address.indexOf(":", 6) + 1)}
                  </span>
                  <span className="text-xs text-muted-foreground">
                    Unavailable
                  </span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <ProjectRepositoryManagement
            identityPubkey={identityPubkey}
            onChange={onRepositoryChange}
            project={project}
            projects={projects}
            repository={repository}
          />
        </div>
      </div>
    </section>
  );
}

export function buildProjectDetailWorkItemCrumb({
  selectedCommitHash,
  selectedIssue,
  selectedPullRequest,
  snapshotCommits,
  setSelectedCommitHash,
  setSelectedIssueId,
  setSelectedPullRequestId,
}: {
  selectedCommitHash: string | null;
  selectedIssue: { id: string; title: string } | null;
  selectedPullRequest: { id: string; title: string } | null;
  snapshotCommits: Array<{ hash: string; subject: string }>;
  setSelectedCommitHash: (hash: string | null) => void;
  setSelectedIssueId: (id: string | null) => void;
  setSelectedPullRequestId: (id: string | null) => void;
}): ProjectDetailWorkItemCrumb | null {
  if (selectedPullRequest) {
    return {
      category: "Pull Request",
      title: selectedPullRequest.title,
      clear: () => setSelectedPullRequestId(null),
    };
  }
  if (selectedIssue) {
    return {
      category: "Issues",
      title: selectedIssue.title,
      clear: () => setSelectedIssueId(null),
    };
  }
  if (selectedCommitHash) {
    const selectedCommit =
      snapshotCommits.find((commit) => commit.hash === selectedCommitHash) ??
      null;
    return {
      category: "Commits",
      title: selectedCommit?.subject ?? selectedCommitHash.slice(0, 7),
      clear: () => setSelectedCommitHash(null),
    };
  }
  return null;
}
