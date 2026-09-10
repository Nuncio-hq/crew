import * as React from "react";
import { ArrowLeft } from "lucide-react";

import type {
  CompanyWikiPage,
  WikiJobState,
  WikiPage,
  WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import type { RelayEvent } from "@/shared/api/types";
import type { OwnerOperationScope } from "@/shared/api/ownerOperations";
import type { WikiRepositoryReadStatus } from "@/shared/api/wikiSnapshot";
import { WikiAskBox } from "@/features/wiki/ui/WikiAskBox";
import { WikiCompanyEditor } from "@/features/wiki/ui/WikiCompanyEditor";
import { WikiHeaderControls } from "@/features/wiki/ui/WikiHeaderControls";
import { WikiMarkdown } from "@/features/wiki/ui/WikiMarkdown";
import { WikiSourceFiles } from "@/features/wiki/ui/WikiSourceFiles";
import { WikiTocMenu } from "@/features/wiki/ui/WikiTocMenu";
import { WikiTocRail } from "@/features/wiki/ui/WikiTocRail";
import { OFFICE_SURFACE } from "@/shared/layout/officeChrome";
import { TopChromeInsetHeader } from "@/shared/layout/TopChromeInsetHeader";

/**
 * Wiki page + TOC (#200 / #205). TOC rail min 200px; below 520px container the
 * rail **collapses** to a hamburger (`WikiTocMenu`). Titles truncate.
 */
export function WikiPageView({
  admin,
  askScope,
  channelId,
  companyError,
  companyPending,
  companyPages,
  door,
  onBack,
  onOpenProject,
  onRetryCompany,
  onRetryRead,
  operationScope,
  owner,
  page,
  pages,
  proposals,
  repoD,
  repoName,
  repoPath,
  repoState,
  readStatus,
  toc,
  workspaceMode,
  recoveryJob,
  onRecoveryRetry,
  onRecoveryReconcile,
  onRecoveryCancel,
  onRegenerate,
  recoveryPending,
  regeneratePending,
}: {
  admin: boolean;
  askScope: "library" | "repo";
  channelId?: string | null;
  companyError?: Error | null;
  companyPending?: boolean;
  companyPages?: CompanyWikiPage[];
  door: "library" | "project";
  onBack?: () => void;
  onOpenProject?: () => void;
  onRetryCompany?: () => void;
  onRetryRead?: () => void;
  operationScope?: OwnerOperationScope;
  owner?: string;
  page: WikiPage | null;
  pages?: WikiPage[];
  proposals?: CompanyWikiPage[];
  repoD?: string;
  repoName: string;
  repoPath?: string | null;
  repoState?: RelayEvent;
  readStatus?: WikiRepositoryReadStatus;
  toc: WikiToc | null;
  workspaceMode?: "git" | "folder";
  recoveryJob?: WikiJobState;
  onRecoveryRetry?: () => void;
  onRecoveryReconcile?: () => void;
  onRecoveryCancel?: () => void;
  onRegenerate?: () => void;
  recoveryPending?: boolean;
  regeneratePending?: boolean;
}) {
  const [activeSlug, setActiveSlug] = React.useState(page?.slug ?? "");
  const [search, setSearch] = React.useState("");
  React.useEffect(() => {
    setActiveSlug(page?.slug ?? "");
  }, [page?.slug]);
  const shown = pages?.find((item) => item.slug === activeSlug) ?? page ?? null;
  const isCompany = repoName === "Company Wiki";
  const emptyCompany = isCompany && !shown && !companyPending && !companyError;
  const readStale = readStatus?.stale ?? false;
  const readUnavailable = readStatus?.unavailable ?? false;

  return (
    <div
      className="@container flex h-full min-h-0 min-w-0"
      data-testid={door === "project" ? "wiki-project-tab" : "wiki-page"}
    >
      <WikiTocRail
        activeSlug={shown?.slug ?? ""}
        filter={search}
        onSelect={setActiveSlug}
        toc={toc}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopChromeInsetHeader
          className="border-b border-border"
          data-office-surface={OFFICE_SURFACE.headerBar}
          data-testid="wiki-header-bar"
        >
          <div className="flex min-w-0 items-center gap-2 px-4 py-2">
            {onBack ? (
              <button
                aria-label="Back to wiki library"
                className="rounded-md p-1 text-muted-foreground hover:text-foreground"
                onClick={onBack}
                type="button"
              >
                <ArrowLeft className="h-4 w-4" />
              </button>
            ) : null}
            <h1 className="min-w-0 flex-1 truncate text-sm font-semibold">
              {shown?.title ?? repoName}
            </h1>
            <WikiTocMenu
              activeSlug={shown?.slug ?? ""}
              onSelect={setActiveSlug}
              toc={toc}
            />
            <WikiHeaderControls
              onOpenProject={admin ? onOpenProject : undefined}
              onSearchChange={setSearch}
              operationScope={operationScope}
              owner={owner}
              repoD={repoD}
              repoPath={repoPath}
              repoState={repoState}
              search={search}
              showCadence={admin && Boolean(repoD)}
              toc={toc}
              workspaceMode={workspaceMode}
              recoveryJob={recoveryJob}
              onRecoveryRetry={onRecoveryRetry}
              onRecoveryReconcile={onRecoveryReconcile}
              onRecoveryCancel={onRecoveryCancel}
              onRegenerate={onRegenerate}
              recoveryPending={recoveryPending}
              regeneratePending={regeneratePending}
            />
          </div>
        </TopChromeInsetHeader>
        <div className="min-h-0 flex-1 overflow-auto px-6 py-4">
          {readStale ? (
            <p
              className="mb-3 text-2xs text-attention"
              data-testid="wiki-read-stale"
            >
              Showing the last verified Wiki.{" "}
              {readStatus?.message ?? "Refresh unavailable."}
            </p>
          ) : null}
          {readUnavailable ? (
            <div
              className="mb-3 rounded-md bg-destructive/10 p-2 text-2xs text-destructive"
              data-testid="wiki-read-unavailable"
            >
              <p>{readStatus?.message ?? "Wiki read unavailable."}</p>
              {onRetryRead ? (
                <button
                  className="mt-2 rounded-md bg-destructive/15 px-2 py-1"
                  data-testid="wiki-retry-read"
                  onClick={onRetryRead}
                  type="button"
                >
                  Retry read
                </button>
              ) : null}
            </div>
          ) : null}
          {isCompany && companyPending && !shown ? (
            <p
              className="text-sm text-muted-foreground"
              data-testid="wiki-company-loading"
            >
              Loading company Wiki…
            </p>
          ) : null}
          {isCompany && companyError ? (
            <div
              className="rounded-md bg-destructive/10 p-3 text-sm text-destructive"
              data-testid="wiki-company-unavailable"
            >
              <p>Company Wiki is unavailable right now.</p>
              {onRetryCompany ? (
                <button
                  className="mt-2 rounded-md bg-destructive/15 px-2 py-1 text-2xs"
                  onClick={onRetryCompany}
                  type="button"
                >
                  Retry company Wiki
                </button>
              ) : null}
            </div>
          ) : null}
          {emptyCompany ? (
            <div data-testid="wiki-company-empty">
              <p className="text-sm text-muted-foreground">
                Company wiki is empty. An agent can propose a page.
              </p>
              <WikiCompanyEditor proposals={proposals ?? []} />
            </div>
          ) : null}
          {!shown &&
          !emptyCompany &&
          !isCompany &&
          readStatus?.state === "missing" ? (
            <p className="text-sm text-muted-foreground">
              This repository has no wiki yet. Generate it from the header.
            </p>
          ) : null}
          {shown ? (
            <>
              <WikiSourceFiles
                files={shown.sourceFiles}
                owner={owner ?? toc?.owner ?? ""}
                pageEvent={shown.event}
                repoD={shown.repoD}
              />
              <WikiMarkdown
                owner={owner ?? toc?.owner ?? ""}
                repoD={shown.repoD}
                source={shown.content}
              />
            </>
          ) : null}
          {companyPages && door === "library" && shown ? (
            <WikiCompanyEditor className="mt-8" proposals={proposals ?? []} />
          ) : null}
        </div>
        <WikiAskBox
          channelId={channelId}
          door={door}
          owner={owner}
          repoD={repoD}
          scopeLabel={
            askScope === "library"
              ? "Asking across wikis + company"
              : `Asking about ${repoName}`
          }
        />
      </div>
    </div>
  );
}
