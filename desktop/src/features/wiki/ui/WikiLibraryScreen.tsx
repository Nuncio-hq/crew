import * as React from "react";
import { BookOpen } from "lucide-react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useProjectsQuery } from "@/features/projects/hooks";
import type { Repository } from "@/features/projects/projectModels";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import { useWikiGenerate } from "@/features/wiki/hooks/useWikiGenerate";
import { useWikiPublicationRecovery } from "@/features/wiki/hooks/useWikiPublicationRecovery";
import { useWikiRefresh } from "@/features/wiki/hooks/useWikiRefresh";
import {
  defaultBranchCommit,
  jobForRepo,
  repoKey,
  wikiFreshness,
  type CompanyWikiPage,
  type WikiPage,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import { classifyWikiRepoProbe } from "@/features/wiki/lib/wikiRepoProbe";
import { getWikiJobs, subscribeWikiJobs } from "@/features/wiki/lib/wikiStore";
import { sameOwnerOperationScope } from "@/shared/api/ownerOperations";
import {
  wikiRepositoryCoordinate,
  type WikiRepositoryReadStatus,
} from "@/shared/api/wikiSnapshot";
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
  const [selected, setSelected] = React.useState<{
    kind: "company" | "repo";
    owner?: string;
    repoD?: string;
    slug?: string;
  } | null>(null);
  const selectedPriority = React.useMemo(
    () =>
      selected?.kind === "repo" && selected.owner && selected.repoD
        ? [wikiRepositoryCoordinate(selected.owner, selected.repoD)]
        : [],
    [selected],
  );
  const eventsQuery = useWikiEventsQuery(undefined, {
    priorityCoordinates: selectedPriority,
  });
  const projectsQuery = useProjectsQuery();
  const generate = useWikiGenerate();
  const operationScope = eventsQuery.scopeQuery.error
    ? undefined
    : eventsQuery.scopeQuery.data;
  const recovery = useWikiPublicationRecovery(operationScope);
  useWikiRefresh();
  const { goProject } = useAppNavigation();
  const jobs = React.useSyncExternalStore(
    subscribeWikiJobs,
    getWikiJobs,
    getWikiJobs,
  );
  const [query, setQuery] = React.useState("");

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
    const companyError =
      eventsQuery.data?.companyError ??
      toQueryError(eventsQuery.companyQuery.error) ??
      toQueryError(eventsQuery.scopeQuery.error);
    return (
      <WikiPageView
        admin
        askScope="library"
        companyPages={publishedCompany}
        companyError={companyError}
        companyPending={
          eventsQuery.companyQuery.isPending || eventsQuery.scopeQuery.isPending
        }
        door="library"
        onBack={() => setSelected(null)}
        onRetryCompany={() => void eventsQuery.companyQuery.refetch()}
        page={companyPageAsWiki(page)}
        proposals={proposals}
        repoName="Company Wiki"
        toc={companyToc(publishedCompany)}
      />
    );
  }

  if (selected?.kind === "repo" && selected.repoD) {
    const repo = repositories.find(
      (item) => item.owner === selected.owner && item.dtag === selected.repoD,
    );
    const toc =
      tocs.find(
        (item) =>
          item.owner === selected.owner && item.repoD === selected.repoD,
      ) ?? null;
    const repoPages = pages.filter(
      (item) =>
        item.event.pubkey === selected.owner && item.repoD === selected.repoD,
    );
    const selectedCoordinate = wikiRepositoryCoordinate(
      selected.owner ?? "",
      selected.repoD,
    );
    const selectedReadStatus =
      eventsQuery.data?.repositoryStatuses[selectedCoordinate];
    const selectedJob = jobForRepo(jobs, selected.owner ?? "", selected.repoD);
    const selectedScope = eventsQuery.scopeQuery.error
      ? undefined
      : eventsQuery.scopeQuery.data;
    const selectedRecovery =
      selectedJob?.operationId &&
      selectedJob.operationRevision !== undefined &&
      selectedJob.scope &&
      selectedScope &&
      sameOwnerOperationScope(selectedJob.scope, selectedScope)
        ? {
            operationId: selectedJob.operationId,
            operationRevision: selectedJob.operationRevision,
            expectedScope: selectedJob.scope,
          }
        : null;
    const page =
      repoPages.find((item) => item.slug === selected.slug) ??
      repoPages[0] ??
      null;
    const repoState = eventsQuery.data?.states.find(
      (event) =>
        event.tags.some((tag) => tag[0] === "d" && tag[1] === selected.repoD) &&
        (event.pubkey === selected.owner ||
          event.tags.some(
            (tag) => tag[0] === "a" && tag[1] === repo?.repoAddress,
          )),
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
        operationScope={selectedScope}
        page={page}
        pages={repoPages}
        repoD={selected.repoD}
        repoName={repo?.name ?? selected.repoD}
        repoPath={repo?.localWorkspacePath}
        repoState={repoState}
        workspaceMode={repo?.workspaceMode}
        readStatus={selectedReadStatus}
        onRetryRead={() => eventsQuery.retryRepository(selectedCoordinate)}
        recoveryJob={selectedJob}
        onRecoveryRetry={
          selectedRecovery
            ? () =>
                recovery.recover({
                  action: "retry",
                  repoKey: repoKey(selected.owner ?? "", selected.repoD ?? ""),
                  ...selectedRecovery,
                })
            : undefined
        }
        onRecoveryReconcile={
          selectedRecovery
            ? () =>
                recovery.recover({
                  action: "reconcile",
                  repoKey: repoKey(selected.owner ?? "", selected.repoD ?? ""),
                  ...selectedRecovery,
                })
            : undefined
        }
        onRecoveryCancel={
          selectedRecovery
            ? () =>
                recovery.recover({
                  action: "cancel",
                  repoKey: repoKey(selected.owner ?? "", selected.repoD ?? ""),
                  ...selectedRecovery,
                })
            : undefined
        }
        onRegenerate={
          // Regeneration captures fresh source, which the native generator can
          // only do from a linked local workspace. Gate the detail view on the
          // same `localWorkspacePath` the library card and Projects tab
          // already require, so an unlinked repository does not offer an
          // action that must fail. Reconcile stays available either way.
          selectedRecovery &&
          repo?.localWorkspacePath &&
          selectedJob?.retiredDependencyId
            ? () =>
                recovery.recover({
                  action: "regenerate",
                  repoKey: repoKey(selected.owner ?? "", selected.repoD ?? ""),
                  repoPath: repo.localWorkspacePath,
                  workspaceMode: repo.workspaceMode,
                  retiredDependencyId: selectedJob.retiredDependencyId,
                  ...selectedRecovery,
                })
            : undefined
        }
        recoveryPending={recovery.isPending}
        regeneratePending={recovery.isPending}
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
        {recovery.error ? (
          <p
            className="mx-auto mb-4 max-w-5xl rounded-md bg-destructive/10 p-2 text-2xs text-destructive"
            data-testid="wiki-recovery-error"
            role="alert"
          >
            Durable Wiki recovery is unavailable: {recovery.error.message}
          </p>
        ) : null}
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
              <div
                className="rounded-xl border border-border bg-card p-4"
                data-testid="wiki-company-card"
              >
                <button
                  className="w-full text-left"
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
                {eventsQuery.data?.companyError ? (
                  <div
                    className="mt-2 text-2xs text-destructive"
                    data-testid="wiki-company-unavailable"
                  >
                    <p>Company Wiki is unavailable right now.</p>
                    <button
                      className="mt-1 rounded-md bg-destructive/15 px-2 py-1"
                      onClick={() => void eventsQuery.companyQuery.refetch()}
                      type="button"
                    >
                      Retry company Wiki
                    </button>
                  </div>
                ) : null}
              </div>
            ) : null}
            {filteredRepos.map((repo) => {
              const toc =
                tocs.find(
                  (item) =>
                    item.owner === repo.owner && item.repoD === repo.dtag,
                ) ?? null;
              const key = repoKey(repo.owner, repo.dtag);
              const job = jobForRepo(jobs, repo.owner, repo.dtag);
              const canWrite = Boolean(
                operationScope &&
                  operationScope.scope.owner.toLowerCase() ===
                    repo.owner.toLowerCase(),
              );
              const recoveryAction =
                canWrite &&
                job?.operationId &&
                job.operationRevision !== undefined &&
                job.scope &&
                operationScope &&
                sameOwnerOperationScope(job.scope, operationScope)
                  ? {
                      operationId: job.operationId,
                      operationRevision: job.operationRevision,
                      expectedScope: job.scope,
                    }
                  : null;
              const coordinate = wikiRepositoryCoordinate(
                repo.owner,
                repo.dtag,
              );
              const readStatus: WikiRepositoryReadStatus | undefined =
                eventsQuery.data?.repositoryStatuses[coordinate];
              const state = eventsQuery.data?.states.find(
                (event) =>
                  event.tags.some(
                    (tag) => tag[0] === "d" && tag[1] === repo.dtag,
                  ) &&
                  (event.pubkey === repo.owner ||
                    event.tags.some(
                      (tag) => tag[0] === "a" && tag[1] === repo.repoAddress,
                    )),
              );
              const freshness =
                job?.status === "generating"
                  ? "generating"
                  : job?.status === "failed"
                    ? "failed"
                    : wikiFreshness(toc, state);
              const tip = defaultBranchCommit(state);
              const probe = classifyWikiRepoProbe({
                jobError: job?.error,
                localWorkspacePath: repo.localWorkspacePath,
                localWorkspaceStatus: repo.localWorkspaceStatus,
                remoteBranch: tip?.branch,
                remoteCommit: tip?.commit,
              });
              return (
                <WikiRepoCard
                  key={repo.id}
                  description={repo.description}
                  freshness={
                    job?.status === "generating" ? "generating" : freshness
                  }
                  probe={probe}
                  readStatus={readStatus}
                  generating={job}
                  name={repo.name}
                  onGenerate={
                    canWrite
                      ? () => {
                          if (!operationScope) return;
                          generate.mutate({
                            owner: repo.owner,
                            repoD: repo.dtag,
                            repoKey: key,
                            repoPath: repo.localWorkspacePath,
                            workspaceMode: repo.workspaceMode,
                            expectedScope: operationScope,
                          });
                        }
                      : undefined
                  }
                  onOpen={() =>
                    setSelected({
                      kind: "repo",
                      owner: repo.owner,
                      repoD: repo.dtag,
                    })
                  }
                  onRetry={() => eventsQuery.retryRepository(coordinate)}
                  onRecoveryRetry={
                    recoveryAction
                      ? () =>
                          recovery.recover({
                            action: "retry",
                            repoKey: key,
                            ...recoveryAction,
                          })
                      : undefined
                  }
                  onRecoveryReconcile={
                    recoveryAction
                      ? () =>
                          recovery.recover({
                            action: "reconcile",
                            repoKey: key,
                            ...recoveryAction,
                          })
                      : undefined
                  }
                  onRecoveryCancel={
                    recoveryAction
                      ? () =>
                          recovery.recover({
                            action: "cancel",
                            repoKey: key,
                            ...recoveryAction,
                          })
                      : undefined
                  }
                  onRegenerate={
                    recoveryAction &&
                    job?.retiredDependencyId &&
                    repo.localWorkspacePath
                      ? () =>
                          recovery.recover({
                            action: "regenerate",
                            repoKey: key,
                            repoPath: repo.localWorkspacePath,
                            workspaceMode: repo.workspaceMode,
                            retiredDependencyId: job.retiredDependencyId,
                            ...recoveryAction,
                          })
                      : undefined
                  }
                  recoveryPending={recovery.isPending}
                  regeneratePending={recovery.isPending}
                  owner={repo.owner}
                  operationScope={operationScope}
                  repoD={repo.dtag}
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

function toQueryError(error: unknown): Error | null {
  if (!error) return null;
  if (error instanceof Error) return error;
  return new Error(typeof error === "string" ? error : "Wiki read failed.");
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
