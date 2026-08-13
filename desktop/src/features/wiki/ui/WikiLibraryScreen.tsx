import * as React from "react";
import { BookOpen } from "lucide-react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useProjectsQuery } from "@/features/projects/hooks";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import { useWikiGenerate } from "@/features/wiki/hooks/useWikiGenerate";
import { useWikiRefresh } from "@/features/wiki/hooks/useWikiRefresh";
import {
  repoKey,
  wikiFreshness,
  type CompanyWikiPage,
  type WikiPage,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import { getWikiJobs, subscribeWikiJobs } from "@/features/wiki/lib/wikiStore";
import { WikiAskBox } from "@/features/wiki/ui/WikiAskBox";
import { WikiCompanyEditor } from "@/features/wiki/ui/WikiCompanyEditor";
import { WikiPageView } from "@/features/wiki/ui/WikiPageView";
import { WikiRepoCard } from "@/features/wiki/ui/WikiRepoCard";
import { TopChromeInsetHeader } from "@/shared/layout/TopChromeInsetHeader";

export function WikiLibraryScreen() {
  const eventsQuery = useWikiEventsQuery();
  const projectsQuery = useProjectsQuery();
  const generate = useWikiGenerate();
  useWikiRefresh();
  const { goProject } = useAppNavigation();
  const jobs = React.useSyncExternalStore(
    subscribeWikiJobs,
    getWikiJobs,
    getWikiJobs,
  );
  const [query, setQuery] = React.useState("");
  const [selected, setSelected] = React.useState<{
    kind: "company" | "repo";
    repoD?: string;
    slug?: string;
  } | null>(null);

  const repositories = React.useMemo(
    () => (projectsQuery.data ?? []).flatMap((project) => project.repositories),
    [projectsQuery.data],
  );
  const tocs = eventsQuery.data?.tocs ?? [];
  const pages = eventsQuery.data?.pages ?? [];
  const company = eventsQuery.data?.company ?? [];
  const publishedCompany = company.filter((page) => !page.proposal);
  const publishedSlugs = new Set(publishedCompany.map((page) => page.slug));
  const proposals = company.filter(
    (page) =>
      page.proposal &&
      !publishedSlugs.has(page.slug.replace(/^_proposal\//, "")),
  );

  const filteredRepos = repositories.filter((repo) => {
    if (!query.trim()) return true;
    const q = query.toLowerCase();
    return (
      repo.name.toLowerCase().includes(q) || repo.dtag.toLowerCase().includes(q)
    );
  });

  if (selected?.kind === "company") {
    const page =
      publishedCompany.find((item) => item.slug === selected.slug) ??
      publishedCompany[0] ??
      null;
    return (
      <WikiPageView
        admin
        askScope="library"
        companyPages={publishedCompany}
        door="library"
        onBack={() => setSelected(null)}
        page={companyPageAsWiki(page)}
        proposals={proposals}
        repoName="Company Wiki"
        toc={companyToc(publishedCompany)}
      />
    );
  }

  if (selected?.kind === "repo" && selected.repoD) {
    const repo = repositories.find((item) => item.dtag === selected.repoD);
    const toc = tocs.find((item) => item.repoD === selected.repoD) ?? null;
    const repoPages = pages.filter((item) => item.repoD === selected.repoD);
    const page =
      repoPages.find((item) => item.slug === selected.slug) ??
      repoPages[0] ??
      null;
    const repoState = eventsQuery.data?.states.find((event) =>
      event.tags.some((tag) => tag[0] === "d" && tag[1] === selected.repoD),
    );
    return (
      <WikiPageView
        admin
        askScope="repo"
        channelId={repo?.channelId ?? null}
        door="library"
        onBack={() => setSelected(null)}
        onOpenProject={() => {
          if (repo) void goProject(repo.id);
        }}
        owner={repo?.owner ?? toc?.owner ?? ""}
        page={page}
        pages={repoPages}
        repoD={selected.repoD}
        repoName={repo?.name ?? selected.repoD}
        repoPath={repo?.localWorkspacePath}
        repoState={repoState}
        toc={toc}
      />
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="wiki-library">
      <TopChromeInsetHeader>
        <div className="flex items-center gap-3 px-4 py-2">
          <BookOpen className="h-4 w-4 text-muted-foreground" />
          <h1 className="text-sm font-semibold">Wiki</h1>
          <input
            aria-label="Search repositories"
            className="ml-auto h-8 w-56 rounded-md border border-border bg-background px-2 text-sm"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search repositories…"
            value={query}
          />
        </div>
      </TopChromeInsetHeader>
      <div className="min-h-0 flex-1 overflow-auto p-6">
        <button
          className="mb-6 flex w-full items-center justify-between rounded-xl border border-border bg-muted/30 px-4 py-3 text-left"
          data-testid="wiki-company-card"
          onClick={() => setSelected({ kind: "company" })}
          type="button"
        >
          <span className="flex items-center gap-2 text-sm font-medium">
            <span className="text-primary">▍</span>
            Company Wiki
          </span>
          <span className="text-2xs text-muted-foreground">
            {publishedCompany.length} pages · curated
          </span>
        </button>
        <h2 className="mb-3 text-sm font-medium text-muted-foreground">
          Repositories
        </h2>
        {filteredRepos.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No repositories yet. Add a Project to generate a wiki.
          </p>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {filteredRepos.map((repo) => {
              const toc = tocs.find((item) => item.repoD === repo.dtag) ?? null;
              const key = repoKey(repo.owner, repo.dtag);
              const job = jobs.get(key);
              const state = eventsQuery.data?.states.find((event) =>
                event.tags.some(
                  (tag) => tag[0] === "d" && tag[1] === repo.dtag,
                ),
              );
              const freshness =
                job?.status === "generating"
                  ? "generating"
                  : job?.status === "failed"
                    ? "failed"
                    : wikiFreshness(toc, state);
              return (
                <WikiRepoCard
                  key={repo.id}
                  emptyRepo={job?.error === "empty-repo"}
                  freshness={
                    job?.status === "generating" ? "generating" : freshness
                  }
                  generating={job}
                  name={repo.name}
                  onGenerate={() => {
                    generate.mutate({
                      owner: repo.owner,
                      repoD: repo.dtag,
                      repoKey: key,
                      repoPath: repo.localWorkspacePath,
                    });
                  }}
                  onOpen={() => setSelected({ kind: "repo", repoD: repo.dtag })}
                  owner={repo.owner}
                  updatedAt={toc?.generatedAt ?? null}
                />
              );
            })}
          </div>
        )}
        <WikiCompanyEditor className="mt-8" proposals={proposals} />
        <div className="mt-10">
          <WikiAskBox
            door="library"
            scopeLabel={`Asking across ${tocs.length} wikis + company`}
          />
        </div>
      </div>
    </div>
  );
}

function companyToc(pages: CompanyWikiPage[]): WikiToc | null {
  if (pages.length === 0) return null;
  return {
    event: pages[0].event,
    repoD: "company",
    owner: pages[0].event.pubkey,
    commit: "",
    branch: "",
    cadence: "manual",
    generatedAt: pages[0].event.created_at,
    sections: [
      {
        id: "company",
        title: "Company",
        pages: pages.map((page) => ({ slug: page.slug, title: page.title })),
      },
    ],
  };
}

function companyPageAsWiki(page: CompanyWikiPage | null): WikiPage | null {
  if (!page) return null;
  return {
    event: page.event,
    repoD: "company",
    slug: page.slug,
    title: page.title,
    section: "company",
    commit: "",
    language: "en",
    sourceFiles: [],
    content: page.content,
  };
}
