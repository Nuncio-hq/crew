import { isCoworkRepository } from "@/features/projects/lib/cowork-project";
import { selectProjectRepository } from "@/features/projects/projectModels";
import { projectRouteRepositoryId } from "./projectDetailHelpers";
import { CrewCoworkProjectScreen } from "@/features/projects/ui/CrewCoworkProjectScreen";
import { ProjectDetailScreen } from "@/features/projects/ui/ProjectDetailScreen";
import { useProjectQuery } from "@/features/projects/hooks";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

export function CrewProjectDetailGate({
  commitHash,
  entityNavigationId,
  filePath,
  issueId,
  projectId,
  pullRequestId,
  repositoryId,
  tab,
  thread,
}: {
  commitHash?: string;
  entityNavigationId?: string;
  filePath?: string;
  issueId?: string;
  projectId: string;
  pullRequestId?: string;
  repositoryId?: string;
  tab?: string;
  thread?: string;
}) {
  const projectQuery = useProjectQuery(projectId);
  if (projectQuery.isPending) {
    return <ViewLoadingFallback kind="projects" />;
  }
  const selectedRepositoryId = projectRouteRepositoryId(
    projectId,
    repositoryId,
  );
  const repository = selectProjectRepository(
    projectQuery.data,
    selectedRepositoryId,
  );
  const hasDetailTarget = Boolean(
    tab || commitHash || filePath || issueId || pullRequestId,
  );
  if (isCoworkRepository(repository) && !hasDetailTarget) {
    return (
      <CrewCoworkProjectScreen
        projectId={projectId}
        repositoryId={selectedRepositoryId}
        threadId={thread}
      />
    );
  }
  return (
    <ProjectDetailScreen
      commitHash={commitHash}
      entityNavigationId={entityNavigationId}
      filePath={filePath}
      issueId={issueId}
      projectId={projectId}
      pullRequestId={pullRequestId}
      repositoryId={repositoryId}
      tab={tab}
    />
  );
}
