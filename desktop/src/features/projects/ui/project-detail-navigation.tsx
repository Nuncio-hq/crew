import { ExternalLink } from "lucide-react";

import type { Project, Repository } from "@/features/projects/hooks";
import type { useProjectRepoPresentation } from "@/features/projects/useProjectRepoHost";
import { Button } from "@/shared/ui/button";
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
    <section className="space-y-3">
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
