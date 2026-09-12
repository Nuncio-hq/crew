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
import type {
  WikiRepositoryReadStatus,
  WikiSnapshotRead,
} from "@/shared/api/wikiSnapshot";
import { wikiRepositoryCoordinate } from "@/shared/api/wikiSnapshot";
import {
  readWikiNavigationState,
  writeWikiNavigationState,
  type WikiNavigationIdentity,
} from "@/features/wiki/lib/wikiNavigationState";
import { WikiAskBox } from "@/features/wiki/ui/WikiAskBox";
import { WikiCompanyEditor } from "@/features/wiki/ui/WikiCompanyEditor";
import { WikiHeaderControls } from "@/features/wiki/ui/WikiHeaderControls";
import { WikiMarkdown } from "@/features/wiki/ui/WikiMarkdown";
import { WikiSourceFiles } from "@/features/wiki/ui/WikiSourceFiles";
import type { WikiSourceOpenRequest } from "@/features/wiki/lib/wikiSourceReferences";
import { WikiTocMenu } from "@/features/wiki/ui/WikiTocMenu";
import { WikiTocRail } from "@/features/wiki/ui/WikiTocRail";
import { useWikiSearch, type WikiSearchResult } from "@/shared/api/wikiSearch";
import { useEscapeKey } from "@/shared/hooks/useEscapeKey";
import { OFFICE_SURFACE } from "@/shared/layout/officeChrome";
import { TopChromeInsetHeader } from "@/shared/layout/TopChromeInsetHeader";
import { cn } from "@/shared/lib/cn";

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
  navigationProjectId,
  owner,
  page,
  pages,
  proposals,
  repoD,
  repoName,
  repoPath,
  repoState,
  readStatus,
  snapshot,
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
  /** Parent Project identity used to scope remembered page/scroll state. */
  navigationProjectId?: string;
  owner?: string;
  page: WikiPage | null;
  pages?: WikiPage[];
  proposals?: CompanyWikiPage[];
  repoD?: string;
  repoName: string;
  repoPath?: string | null;
  repoState?: RelayEvent;
  readStatus?: WikiRepositoryReadStatus;
  snapshot?: WikiSnapshotRead | null;
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
  const isCompany = repoName === "Company Wiki";
  const navigationIdentity =
    React.useMemo<WikiNavigationIdentity | null>(() => {
      if (!operationScope || !navigationProjectId) return null;
      const repositoryCoordinate = isCompany
        ? "company"
        : owner && repoD
          ? wikiRepositoryCoordinate(owner, repoD)
          : "";
      if (!repositoryCoordinate) return null;
      return {
        community: operationScope.scope.community,
        viewer: operationScope.scope.owner,
        projectId: navigationProjectId,
        repositoryCoordinate,
        surface: door,
      };
    }, [door, isCompany, navigationProjectId, operationScope, owner, repoD]);
  const navigationKey = React.useMemo(
    () => (navigationIdentity ? JSON.stringify(navigationIdentity) : null),
    [navigationIdentity],
  );
  const [activeSlug, setActiveSlug] = React.useState(page?.slug ?? "");
  const [search, setSearch] = React.useState("");
  const [navigationNotice, setNavigationNotice] = React.useState<string | null>(
    null,
  );
  const [sourcePaneRequest, setSourcePaneRequest] = React.useState<{
    pageId: string;
    path: string;
    startLine: number;
    endLine: number;
  } | null>(null);
  const [sourceNotice, setSourceNotice] = React.useState<string | null>(null);
  const scrollRef = React.useRef<HTMLDivElement>(null);
  const loadedNavigationKeyRef = React.useRef<string | null>(null);
  const restoreRequestRef = React.useRef<{
    key: string;
    pageId: string;
    scrollTop: number;
  } | null>(null);
  const restoreGenerationRef = React.useRef(0);
  const restoreTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const restoreObserverRef = React.useRef<ResizeObserver | null>(null);
  const restoringRef = React.useRef(false);
  const lastRestoreScrollTopRef = React.useRef<number | null>(null);
  const sourceTriggerRef = React.useRef<HTMLElement | null>(null);
  const availablePages = React.useMemo(
    () => pages ?? (page ? [page] : []),
    [page, pages],
  );
  const shown = availablePages.find((item) => item.slug === activeSlug) ?? null;
  const navigationSnapshotRef = React.useRef<{
    key: string;
    identity: WikiNavigationIdentity;
    page: WikiPage;
    activeSlug: string;
    scrollTop: number;
  } | null>(null);

  const stopScrollRestore = React.useCallback(() => {
    restoreGenerationRef.current += 1;
    restoringRef.current = false;
    scrollRef.current?.removeAttribute("data-wiki-scroll-restore-pending");
    restoreRequestRef.current = null;
    lastRestoreScrollTopRef.current = null;
    if (restoreTimerRef.current !== null) {
      clearTimeout(restoreTimerRef.current);
      restoreTimerRef.current = null;
    }
    restoreObserverRef.current?.disconnect();
    restoreObserverRef.current = null;
  }, []);

  const persistCurrentNavigation = React.useCallback(() => {
    if (!navigationIdentity) return;
    const current = availablePages.find((item) => item.slug === activeSlug);
    const scrollTop = scrollRef.current?.scrollTop ?? 0;
    writeWikiNavigationState(navigationIdentity, {
      pageId: current?.event.id || null,
      pageSlug: current?.slug ?? (activeSlug || null),
      scrollTop,
    });
    if (navigationKey && current) {
      navigationSnapshotRef.current = {
        key: navigationKey,
        identity: navigationIdentity,
        page: current,
        activeSlug,
        scrollTop,
      };
    }
  }, [activeSlug, availablePages, navigationIdentity, navigationKey]);

  const startScrollRestore = React.useCallback(
    (request: { key: string; pageId: string; scrollTop: number }) => {
      stopScrollRestore();
      const generation = restoreGenerationRef.current;
      restoringRef.current = true;
      scrollRef.current?.setAttribute("data-wiki-scroll-restore-pending", "");
      restoreRequestRef.current = request;
      const deadline = Date.now() + 1_000;
      const scheduleRetry = () => {
        if (restoreTimerRef.current !== null) return;
        const remaining = deadline - Date.now();
        if (remaining <= 0) {
          stopScrollRestore();
          return;
        }
        restoreTimerRef.current = setTimeout(
          () => {
            restoreTimerRef.current = null;
            tryRestore();
          },
          Math.min(100, remaining),
        );
      };
      function tryRestore() {
        if (
          generation !== restoreGenerationRef.current ||
          !restoringRef.current ||
          restoreRequestRef.current !== request
        ) {
          return;
        }
        const element = scrollRef.current;
        if (!element) {
          scheduleRetry();
          return;
        }
        // jsdom and a newly mounted WebKit node can report zero dimensions
        // before layout. Preserve the requested value until a real range is
        // available; once layout reports dimensions, clamp to the browser's
        // reachable range and keep retrying as content grows.
        const hasLayout = element.scrollHeight > 0 || element.clientHeight > 0;
        const maxScrollTop = hasLayout
          ? Math.max(0, element.scrollHeight - element.clientHeight)
          : request.scrollTop;
        const nextScrollTop = Math.min(request.scrollTop, maxScrollTop);
        element.scrollTop = nextScrollTop;
        lastRestoreScrollTopRef.current = element.scrollTop;
        const targetReached =
          request.scrollTop === 0 ||
          (hasLayout && element.scrollTop >= request.scrollTop);
        if (targetReached) {
          stopScrollRestore();
          return;
        }
        scheduleRetry();
      }
      const element = scrollRef.current;
      if (element && typeof ResizeObserver !== "undefined") {
        const observer = new ResizeObserver(tryRestore);
        const content = element.firstElementChild;
        observer.observe(content instanceof HTMLElement ? content : element);
        restoreObserverRef.current = observer;
      }
      tryRestore();
    },
    [stopScrollRestore],
  );

  const openSourcePane = React.useCallback(
    (request: WikiSourceOpenRequest, trigger?: HTMLElement | null) => {
      const current = shown;
      if (
        !current ||
        request.startLine === undefined ||
        request.endLine === undefined
      ) {
        setSourceNotice(
          "This source citation is unavailable for the published page revision.",
        );
        return;
      }
      stopScrollRestore();
      sourceTriggerRef.current =
        trigger ??
        (typeof document !== "undefined" &&
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null);
      setSourceNotice(null);
      setSourcePaneRequest({
        pageId: current.event.id,
        path: request.path,
        startLine: request.startLine,
        endLine: request.endLine,
      });
    },
    [shown, stopScrollRestore],
  );

  const closeSourcePane = React.useCallback(() => {
    const trigger = sourceTriggerRef.current;
    sourceTriggerRef.current = null;
    setSourcePaneRequest(null);
    setSourceNotice(null);
    queueMicrotask(() => {
      if (trigger?.isConnected) {
        trigger.focus({ preventScroll: true });
      }
    });
  }, []);
  useEscapeKey(closeSourcePane, sourcePaneRequest !== null);

  const selectPage = React.useCallback(
    (slug: string) => {
      const next = availablePages.find((item) => item.slug === slug);
      if (!next) return;
      stopScrollRestore();
      if (sourcePaneRequest) {
        sourceTriggerRef.current = null;
        setSourcePaneRequest(null);
      }
      setSourceNotice(null);
      setNavigationNotice(null);
      setActiveSlug(slug);
      if (navigationIdentity) {
        writeWikiNavigationState(navigationIdentity, {
          pageId: next.event.id || null,
          pageSlug: next.slug,
          scrollTop: 0,
        });
        if (navigationKey) {
          navigationSnapshotRef.current = {
            key: navigationKey,
            identity: navigationIdentity,
            page: next,
            activeSlug: slug,
            scrollTop: 0,
          };
        }
      }
    },
    [
      availablePages,
      navigationIdentity,
      navigationKey,
      sourcePaneRequest,
      stopScrollRestore,
    ],
  );

  const handleContentScroll = React.useCallback(() => {
    if (restoringRef.current) {
      const currentScrollTop = scrollRef.current?.scrollTop ?? 0;
      if (currentScrollTop === lastRestoreScrollTopRef.current) return;
      stopScrollRestore();
    }
    if (!navigationIdentity) return;
    const current = availablePages.find((item) => item.slug === activeSlug);
    const scrollTop = scrollRef.current?.scrollTop ?? 0;
    writeWikiNavigationState(navigationIdentity, {
      pageId: current?.event.id || null,
      pageSlug: current?.slug ?? (activeSlug || null),
      scrollTop,
    });
    if (navigationKey && current) {
      navigationSnapshotRef.current = {
        key: navigationKey,
        identity: navigationIdentity,
        page: current,
        activeSlug,
        scrollTop,
      };
    }
  }, [
    activeSlug,
    availablePages,
    navigationIdentity,
    navigationKey,
    stopScrollRestore,
  ]);

  const cancelRestoreFromUserInput = React.useCallback(() => {
    if (!restoringRef.current) return;
    stopScrollRestore();
    persistCurrentNavigation();
  }, [persistCurrentNavigation, stopScrollRestore]);
  const handleScrollKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (
        event.key === "ArrowDown" ||
        event.key === "ArrowUp" ||
        event.key === "PageDown" ||
        event.key === "PageUp" ||
        event.key === "Home" ||
        event.key === "End" ||
        event.key === " "
      ) {
        cancelRestoreFromUserInput();
      }
    },
    [cancelRestoreFromUserInput],
  );

  React.useEffect(() => {
    if (!navigationKey) {
      loadedNavigationKeyRef.current = null;
      return;
    }
    if (availablePages.length === 0) return;
    const firstLoad = loadedNavigationKeyRef.current !== navigationKey;
    if (firstLoad) {
      loadedNavigationKeyRef.current = navigationKey;
      const saved = navigationIdentity
        ? readWikiNavigationState(navigationIdentity)
        : null;
      const savedPage = saved
        ? saved.pageId
          ? availablePages.find((item) => item.event.id === saved.pageId)
          : saved.pageSlug
            ? availablePages.find((item) => item.slug === saved.pageSlug)
            : null
        : null;
      const fallback =
        availablePages.find((item) => item.slug === page?.slug) ??
        availablePages[0];
      const selected = savedPage ?? fallback;
      if (selected) {
        setActiveSlug(selected.slug);
        if (!savedPage && saved?.pageId) {
          setNavigationNotice(
            `The saved Wiki page is no longer available. Showing “${selected.title}”.`,
          );
        }
        if (navigationIdentity) {
          writeWikiNavigationState(navigationIdentity, {
            pageId: selected.event.id || null,
            pageSlug: selected.slug,
            scrollTop: savedPage ? (saved?.scrollTop ?? 0) : 0,
          });
          restoreRequestRef.current = {
            key: navigationKey,
            pageId: selected.event.id,
            scrollTop: savedPage ? (saved?.scrollTop ?? 0) : 0,
          };
          if (
            selected.event.id === shown?.event.id &&
            selected.slug === activeSlug
          ) {
            startScrollRestore(restoreRequestRef.current);
          }
        }
      }
      return;
    }
    if (availablePages.some((item) => item.slug === activeSlug)) return;
    const fallback = availablePages[0];
    if (!fallback) return;
    setActiveSlug(fallback.slug);
    setNavigationNotice(
      `This Wiki page is no longer available. Showing “${fallback.title}”.`,
    );
    if (navigationIdentity) {
      writeWikiNavigationState(navigationIdentity, {
        pageId: fallback.event.id || null,
        pageSlug: fallback.slug,
        scrollTop: 0,
      });
    }
  }, [
    activeSlug,
    availablePages,
    navigationIdentity,
    navigationKey,
    page?.slug,
    shown,
    startScrollRestore,
  ]);

  React.useLayoutEffect(() => {
    if (!navigationIdentity || !navigationKey || !shown) return;
    const request = restoreRequestRef.current;
    if (
      !request ||
      request.key !== navigationKey ||
      request.pageId !== shown.event.id
    ) {
      return;
    }
    restoreRequestRef.current = null;
    startScrollRestore(request);
  }, [navigationIdentity, navigationKey, shown, startScrollRestore]);

  React.useEffect(() => {
    return () => {
      stopScrollRestore();
    };
  }, [stopScrollRestore]);

  React.useEffect(() => {
    if (sourcePaneRequest && sourcePaneRequest.pageId !== shown?.event.id) {
      closeSourcePane();
    }
  }, [closeSourcePane, shown?.event.id, sourcePaneRequest]);

  React.useEffect(() => {
    if (!navigationKey) return;
    return () => {
      const current = navigationSnapshotRef.current;
      if (!current || current.key !== navigationKey) return;
      writeWikiNavigationState(current.identity, {
        pageId: current.page.event.id || null,
        pageSlug: current.page.slug || current.activeSlug || null,
        scrollTop: current.scrollTop,
      });
      if (navigationSnapshotRef.current?.key === navigationKey) {
        navigationSnapshotRef.current = null;
      }
    };
  }, [navigationKey]);

  const emptyCompany = isCompany && !shown && !companyPending && !companyError;
  const readStale = readStatus?.stale ?? false;
  const readUnavailable = readStatus?.unavailable ?? false;
  const searchQuery = useWikiSearch({
    coordinate:
      owner && repoD ? wikiRepositoryCoordinate(owner, repoD) : undefined,
    query: search,
    scope: isCompany ? undefined : operationScope,
    snapshot: isCompany ? undefined : snapshot,
  });

  return (
    <div
      className="@container flex h-full min-h-0 min-w-0"
      data-testid={door === "project" ? "wiki-project-tab" : "wiki-page"}
    >
      {!sourcePaneRequest ? (
        <WikiTocRail
          activeSlug={shown?.slug ?? ""}
          filter={search}
          onSelect={selectPage}
          toc={toc}
        />
      ) : null}
      <div className="flex min-w-0 flex-1 flex-col">
        <TopChromeInsetHeader
          className={cn("border-b border-border", door === "project" && "z-20")}
          data-office-surface={OFFICE_SURFACE.headerBar}
          data-testid="wiki-header-bar"
        >
          <div className="flex min-w-0 flex-wrap items-center gap-2 px-4 py-2">
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
            <h1 className="min-w-0 flex-1 basis-40 truncate text-sm font-semibold">
              {shown?.title ?? repoName}
            </h1>
            {!sourcePaneRequest ? (
              <WikiTocMenu
                activeSlug={shown?.slug ?? ""}
                onSelect={selectPage}
                toc={toc}
              />
            ) : null}
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
        <section
          aria-label="Wiki page content"
          className="min-h-0 flex-1 overflow-auto px-6 py-4"
          data-testid="wiki-page-scroll"
          onScroll={handleContentScroll}
          onKeyDown={handleScrollKeyDown}
          onPointerDown={cancelRestoreFromUserInput}
          onTouchStart={cancelRestoreFromUserInput}
          onWheel={cancelRestoreFromUserInput}
          ref={scrollRef}
        >
          <div
            className={
              sourcePaneRequest
                ? "flex min-w-0 flex-col gap-4 [@container(min-width:48rem)]:flex-row"
                : "min-w-0"
            }
          >
            <article
              className={
                sourcePaneRequest
                  ? "hidden min-w-0 flex-1 [@container(min-width:48rem)]:block"
                  : "min-w-0"
              }
              data-testid="wiki-page-article"
            >
              {navigationNotice ? (
                <p
                  aria-live="polite"
                  className="mb-3 rounded-md bg-muted/40 p-2 text-2xs text-muted-foreground"
                  data-testid="wiki-navigation-fallback"
                  role="status"
                >
                  {navigationNotice}
                </p>
              ) : null}
              {sourceNotice ? (
                <p
                  aria-live="polite"
                  className="mb-3 rounded-md bg-muted/40 p-2 text-2xs text-muted-foreground"
                  data-testid="wiki-source-unavailable"
                  role="status"
                >
                  {sourceNotice}
                </p>
              ) : null}
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
              {!isCompany && search.trim() ? (
                <WikiSearchResultsPanel
                  query={search}
                  searchQuery={searchQuery}
                  onSelect={(slug) => {
                    selectPage(slug);
                    setSearch("");
                  }}
                  hasSnapshot={snapshot?.state === "complete"}
                />
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
                  <div className={sourcePaneRequest ? "hidden" : undefined}>
                    <WikiSourceFiles
                      files={shown.sourceFiles}
                      owner={owner ?? toc?.owner ?? ""}
                      onOpenPane={openSourcePane}
                      pageEvent={shown.event}
                      repoD={shown.repoD}
                      operationScope={operationScope}
                    />
                  </div>
                  <WikiMarkdown
                    owner={owner ?? toc?.owner ?? ""}
                    onOpenSource={openSourcePane}
                    onSourceUnavailable={setSourceNotice}
                    pageEvent={shown.event}
                    repoD={shown.repoD}
                    source={shown.content}
                  />
                </>
              ) : null}
              {companyPages && door === "library" && shown ? (
                <WikiCompanyEditor
                  className="mt-8"
                  proposals={proposals ?? []}
                />
              ) : null}
            </article>
            {sourcePaneRequest && shown ? (
              <aside
                className="min-w-0 [@container(min-width:48rem)]:w-[min(42%,32rem)] [@container(min-width:48rem)]:shrink-0"
                data-testid="wiki-source-pane-region"
              >
                <WikiSourceFiles
                  files={shown.sourceFiles}
                  initialRequest={sourcePaneRequest}
                  mode="pane"
                  onClosePane={closeSourcePane}
                  owner={owner ?? toc?.owner ?? ""}
                  pageEvent={shown.event}
                  repoD={shown.repoD}
                  operationScope={operationScope}
                />
              </aside>
            ) : null}
          </div>
        </section>
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

function WikiSearchResultsPanel({
  hasSnapshot,
  onSelect,
  query,
  searchQuery,
}: {
  hasSnapshot: boolean;
  onSelect: (slug: string) => void;
  query: string;
  searchQuery: ReturnType<typeof useWikiSearch>;
}) {
  if (!hasSnapshot) {
    return (
      <p
        className="mb-4 rounded-md bg-muted/40 p-3 text-sm text-muted-foreground"
        data-testid="wiki-search-unavailable"
      >
        Body search is available after this repository&apos;s verified Wiki
        snapshot finishes loading.
      </p>
    );
  }
  if (
    searchQuery.isDebouncing ||
    searchQuery.isPending ||
    searchQuery.isFetching
  ) {
    return (
      <p
        aria-live="polite"
        className="mb-4 text-sm text-muted-foreground"
        data-testid="wiki-search-loading"
      >
        Searching Wiki bodies…
      </p>
    );
  }
  if (searchQuery.isError) {
    return (
      <p
        className="mb-4 rounded-md bg-destructive/10 p-3 text-sm text-destructive"
        data-testid="wiki-search-error"
        role="alert"
      >
        Body search is unavailable:{" "}
        {searchQuery.error?.message ?? "Wiki search failed."}
      </p>
    );
  }
  const data = searchQuery.data;
  if (data?.truncated && data.results.length === 0) {
    return (
      <p
        className="mb-4 rounded-md bg-muted/40 p-3 text-sm text-muted-foreground"
        data-testid="wiki-search-limited"
      >
        Search returned more matches than can be shown. Refine the query before
        treating this as a no-match result.
      </p>
    );
  }
  if (!data || data.results.length === 0) {
    return (
      <p
        className="mb-4 text-sm text-muted-foreground"
        data-testid="wiki-search-empty"
      >
        No Wiki page body contains “{query.trim()}”.
      </p>
    );
  }
  return (
    <section
      aria-label="Wiki body search results"
      className="mb-5 rounded-lg border border-border bg-card p-3"
      data-testid="wiki-search-results"
    >
      <div className="mb-2 text-2xs font-medium uppercase tracking-wide text-muted-foreground">
        {data.results.length} matching{" "}
        {data.results.length === 1 ? "page" : "pages"}
      </div>
      <div className="space-y-1">
        {data.results.map((result) => (
          <WikiSearchResultButton
            key={result.pageEventId}
            onSelect={onSelect}
            result={result}
          />
        ))}
      </div>
      {data.truncated ? (
        <p
          className="mt-2 text-2xs text-attention"
          data-testid="wiki-search-limited"
        >
          Showing the first {data.results.length} matches. Refine the query for
          more.
        </p>
      ) : null}
    </section>
  );
}

function WikiSearchResultButton({
  onSelect,
  result,
}: {
  onSelect: (slug: string) => void;
  result: WikiSearchResult;
}) {
  return (
    <button
      className="block w-full rounded-md px-2 py-1.5 text-left hover:bg-muted"
      data-testid={`wiki-search-result-${result.slug}`}
      onClick={() => onSelect(result.slug)}
      type="button"
    >
      <span className="block text-sm font-medium text-foreground">
        {result.title}
      </span>
      <span className="block truncate text-2xs text-muted-foreground">
        {result.section} · {result.excerpt}
      </span>
    </button>
  );
}
