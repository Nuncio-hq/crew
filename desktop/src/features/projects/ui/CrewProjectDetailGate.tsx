import { isCoworkProject } from "@/features/projects/lib/cowork-project";
import { CrewCoworkProjectScreen } from "@/features/projects/ui/CrewCoworkProjectScreen";
import { ProjectDetailScreen } from "@/features/projects/ui/ProjectDetailScreen";
import { useProjectQuery } from "@/features/projects/hooks";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

export function CrewProjectDetailGate({
  commitHash,
  issueId,
  projectId,
  pullRequestId,
  repositoryId,
  tab,
  thread,
}: {
  commitHash?: string;
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
  if (isCoworkProject(projectQuery.data)) {
    return <CrewCoworkProjectScreen projectId={projectId} threadId={thread} />;
  }
  return (
    <ProjectDetailScreen
      commitHash={commitHash}
      issueId={issueId}
      projectId={projectId}
      pullRequestId={pullRequestId}
      repositoryId={repositoryId}
      tab={tab}
    />
  );
}
