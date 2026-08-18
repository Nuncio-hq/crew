import * as React from "react";
import { BookOpen } from "lucide-react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useProjectsQuery } from "@/features/projects/hooks";
import type { Repository } from "@/features/projects/projectModels";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import { useWikiGenerate } from "@/features/wiki/hooks/useWikiGenerate";
import { useWikiRefresh } from "@/features/wiki/hooks/useWikiRefresh";
import {
  jobForRepo,
  repoKey,
  wikiFreshness,
  type CompanyWikiPage,
  type WikiPage,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import { getWikiJobs, subscribeWikiJobs } from "@/features/wiki/lib/wikiStore";
import { WikiPageView } from "@/features/wiki/ui/WikiPageView";
import { WikiRepoCard } from "@/features/wiki/ui/WikiRepoCard";
import {
  OFFICE_FIELD_BOX_CLASS,
  OFFICE_FIELD_CONTROL_CLASS,
  OFFICE_SURFACE,
} from "@/shared/layout/officeChrome";
import { TopChromeInsetHeader } from "@/shared/layout/TopChromeInsetHeader";
import { cn } from "@/shared/lib/cn";

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

  const repositories = React.useMemo(() => {
    const seen = new Set<string>();
    const unique: Repository[] = [];
    for (const repo of (projectsQuery.data ?? []).flatMap(
      (project) => project.repositories,
    )) {
      if (seen.has(repo.repoAddress)) continue;
      seen.add(repo.repoAddress);
      unique.push(repo);
    }
    return unique;
  }, [projectsQuery.data]);
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

  const showCompanyCard =
    !query.trim() || "company wiki".includes(query.trim().toLowerCase());

  return (
    <div className="flex h-full min-h-0 flex-col" data-testid="wiki-library">
      <TopChromeInsetHeader
        className="border-b border-border"
        data-office-surface={OFFICE_SURFACE.headerBar}
        data-testid="wiki-header-bar"
      >
        <div className="flex items-center gap-3 px-4 py-2">
          <BookOpen className="h-4 w-4 text-muted-foreground" />
          <h1 className="text-sm font-semibold">Wiki</h1>
        </div>
      </TopChromeInsetHeader>
      <div className="min-h-0 flex-1 overflow-auto px-6 py-8">
        <div className="mx-auto flex w-full max-w-4xl flex-col items-center">
          <h2 className="mb-4 text-center text-xl font-semibold">
            Which repo would you like to understand?
          </h2>
          <div
            className={cn(OFFICE_FIELD_BOX_CLASS, "mb-8 flex w-full max-w-xl")}
            data-office-surface={OFFICE_SURFACE.fieldBox}
            data-testid="wiki-home-search"
          >
            <input
              aria-label="Search repositories"
              className={cn(
                OFFICE_FIELD_CONTROL_CLASS,
                "h-10 w-full px-3 text-sm",
              )}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search for repositories"
              value={query}
            />
          </div>
        </div>
        {filteredRepos.length === 0 && !showCompanyCard ? (
          <p className="text-sm text-muted-foreground">
            No repositories yet. Add a Project to generate a wiki.
          </p>
        ) : (
          <div className="mx-auto grid max-w-5xl gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {showCompanyCard ? (
              <button
                className="rounded-xl border border-border bg-card p-4 text-left"
                data-testid="wiki-company-card"
                onClick={() => setSelected({ kind: "company" })}
                type="button"
              >
                <div className="text-sm font-medium">Company Wiki</div>
                <p className="mt-2 text-sm text-muted-foreground">
                  {publishedCompany.length > 0
                    ? `${publishedCompany.length} generated pages`
                    : "Generated company pages, when agents propose them."}
                </p>
              </button>
            ) : null}
            {filteredRepos.map((repo) => {
              const toc = tocs.find((item) => item.repoD === repo.dtag) ?? null;
              const key = repoKey(repo.owner, repo.dtag);
              const job = jobForRepo(jobs, repo.owner, repo.dtag);
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
                  description={repo.description}
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
