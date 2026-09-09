import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useChannelsQuery } from "@/features/channels/hooks";
import { useProjectQuery } from "@/features/projects/hooks";
import { Button } from "@/shared/ui/button";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";
import { CrewProjectOverview } from "./CrewProjectOverview";
import { ProjectChannelLinkControl } from "./ProjectChannelLinkControl";

/** Project landing page backed by the existing relay query projections. */
export function CrewProjectOverviewScreen({
  projectId,
  repositoryId,
}: {
  projectId: string;
  repositoryId?: string;
}) {
  const projectQuery = useProjectQuery(projectId);
  const channelsQuery = useChannelsQuery();
  const { goProjects, goProject, goChannel } = useAppNavigation();
  if (projectQuery.isPending) {
    return <ViewLoadingFallback kind="projects" />;
  }
  if (projectQuery.isError || !projectQuery.data) {
    return (
      <section className="flex flex-1 flex-col items-center justify-center gap-3 p-6">
        <p role="status">
          {projectQuery.isError
            ? "Could not load this project."
            : "This project could not be found."}
        </p>
        <Button onClick={() => void projectQuery.refetch()} variant="outline">
          Retry
        </Button>
        <Button onClick={() => void goProjects()} variant="ghost">
          Back to Projects
        </Button>
      </section>
    );
  }
  return (
    <CrewProjectOverview
      project={projectQuery.data}
      channels={channelsQuery.data ?? []}
      channelAction={
        <ProjectChannelLinkControl
          project={projectQuery.data}
          channels={channelsQuery.data ?? []}
        />
      }
      channelStatus={
        channelsQuery.isError ? (
          <p role="status" className="text-sm text-muted-foreground">
            Could not load channel details. Existing links are preserved.
            <Button
              onClick={() => void channelsQuery.refetch()}
              variant="ghost"
              size="sm"
            >
              Retry channel details
            </Button>
          </p>
        ) : channelsQuery.isPending ? (
          <p role="status" className="text-sm text-muted-foreground">
            Loading channel details…
          </p>
        ) : null
      }
      workspaceAction={null}
      workspaceManagement={(address) => (
        <Button
          onClick={() =>
            void goProject(projectId, {
              repositoryId: address,
              tab: "overview",
            })
          }
          variant="outline"
          size="sm"
        >
          Open workspace details
        </Button>
      )}
      onOpenChannel={(channelId) => void goChannel(channelId)}
      onOpenWiki={() =>
        void goProject(projectId, { repositoryId, tab: "wiki" })
      }
      onOpenProjects={() => void goProjects()}
    />
  );
}
