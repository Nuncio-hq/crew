import { ArrowLeft, FolderGit2 } from "lucide-react";

import type { Project } from "@/features/projects/hooks";
import { Button } from "@/shared/ui/button";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";
import { UnavailableProjectRepositories } from "./UnavailableProjectRepositories";

export function ProjectDetailScreenLoading() {
  return <ViewLoadingFallback kind="projects" />;
}

export function ProjectDetailScreenError({
  onGoProjects,
  onRetry,
}: {
  onGoProjects: () => void;
  onRetry: () => void;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-16 text-center">
      <FolderGit2 className="h-10 w-10 text-muted-foreground/40" />
      <p className="text-sm text-red-400">Failed to load project</p>
      <div className="flex items-center gap-2">
        <Button onClick={onRetry} size="sm" variant="outline">
          Retry
        </Button>
        <Button onClick={onGoProjects} size="sm" variant="ghost">
          <ArrowLeft className="mr-1.5 h-4 w-4" />
          Back to Projects
        </Button>
      </div>
    </div>
  );
}

export function ProjectDetailScreenNotFound({
  onGoProjects,
}: {
  onGoProjects: () => void;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-16 text-center">
      <FolderGit2 className="h-10 w-10 text-muted-foreground/40" />
      <p className="text-sm text-muted-foreground">
        This project could not be found.
      </p>
      <Button onClick={onGoProjects} size="sm" variant="outline">
        <ArrowLeft className="mr-1.5 h-4 w-4" />
        Back to Projects
      </Button>
    </div>
  );
}

export function ProjectDetailScreenNoRepository({
  project,
}: {
  project: Project;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-16 text-center">
      <FolderGit2 className="h-10 w-10 text-muted-foreground/40" />
      <p className="text-sm font-medium text-foreground">{project.name}</p>
      <p className="text-sm text-muted-foreground">
        This project does not have any available repositories yet.
      </p>
      <UnavailableProjectRepositories project={project} />
    </div>
  );
}
