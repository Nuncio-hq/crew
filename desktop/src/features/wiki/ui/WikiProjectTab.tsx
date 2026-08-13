import { WikiPageView } from "@/features/wiki/ui/WikiPageView";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import type { Repository } from "@/features/projects/hooks";

export function WikiProjectTab({ project }: { project: Repository }) {
  const eventsQuery = useWikiEventsQuery();
  const toc =
    eventsQuery.data?.tocs.find((item) => item.repoD === project.dtag) ?? null;
  const pages =
    eventsQuery.data?.pages.filter((item) => item.repoD === project.dtag) ?? [];
  const repoState = eventsQuery.data?.states.find((event) =>
    event.tags.some((tag) => tag[0] === "d" && tag[1] === project.dtag),
  );
  return (
    <WikiPageView
      admin={false}
      askScope="repo"
      channelId={project.channelId ?? null}
      door="project"
      owner={project.owner}
      page={pages[0] ?? null}
      pages={pages}
      repoD={project.dtag}
      repoName={project.name}
      repoPath={project.localWorkspacePath}
      repoState={repoState}
      toc={toc}
    />
  );
}
