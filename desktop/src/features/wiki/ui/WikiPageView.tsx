import * as React from "react";
import { ArrowLeft } from "lucide-react";

import type {
  CompanyWikiPage,
  WikiPage,
  WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import type { RelayEvent } from "@/shared/api/types";
import { WikiAskBox } from "@/features/wiki/ui/WikiAskBox";
import { WikiCompanyEditor } from "@/features/wiki/ui/WikiCompanyEditor";
import { WikiHeaderControls } from "@/features/wiki/ui/WikiHeaderControls";
import { WikiMarkdown } from "@/features/wiki/ui/WikiMarkdown";
import { WikiSourceFiles } from "@/features/wiki/ui/WikiSourceFiles";
import { WikiTocRail } from "@/features/wiki/ui/WikiTocRail";

export function WikiPageView({
  admin,
  askScope,
  channelId,
  companyPages,
  door,
  onBack,
  onOpenProject,
  owner,
  page,
  pages,
  proposals,
  repoD,
  repoName,
  repoPath,
  repoState,
  toc,
}: {
  admin: boolean;
  askScope: "library" | "repo";
  channelId?: string | null;
  companyPages?: CompanyWikiPage[];
  door: "library" | "project";
  onBack?: () => void;
  onOpenProject?: () => void;
  owner?: string;
  page: WikiPage | null;
  pages?: WikiPage[];
  proposals?: CompanyWikiPage[];
  repoD?: string;
  repoName: string;
  repoPath?: string | null;
  repoState?: RelayEvent;
  toc: WikiToc | null;
}) {
  const [activeSlug, setActiveSlug] = React.useState(page?.slug ?? "");
  const [search, setSearch] = React.useState("");
  React.useEffect(() => {
    setActiveSlug(page?.slug ?? "");
  }, [page?.slug]);
  const shown = pages?.find((item) => item.slug === activeSlug) ?? page ?? null;
  const emptyCompany = repoName === "Company Wiki" && !shown;

  return (
    <div
      className="flex h-full min-h-0"
      data-testid={door === "project" ? "wiki-project-tab" : "wiki-page"}
    >
      <WikiTocRail
        activeSlug={shown?.slug ?? ""}
        filter={search}
        onSelect={setActiveSlug}
        toc={toc}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2">
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
          <WikiHeaderControls
            onOpenProject={admin ? onOpenProject : undefined}
            onSearchChange={setSearch}
            owner={owner}
            repoD={repoD}
            repoPath={repoPath}
            repoState={repoState}
            search={search}
            showCadence={admin && Boolean(repoD)}
            toc={toc}
          />
        </div>
        <div className="min-h-0 flex-1 overflow-auto px-6 py-4">
          {emptyCompany ? (
            <div data-testid="wiki-company-empty">
              <p className="text-sm text-muted-foreground">
                Company wiki is empty. Create the first page, or let an agent
                propose one.
              </p>
              <WikiCompanyEditor proposals={proposals ?? []} />
            </div>
          ) : null}
          {!shown && !emptyCompany ? (
            <p className="text-sm text-muted-foreground">
              This repository has no wiki yet. Generate it from the library.
            </p>
          ) : null}
          {shown ? (
            <>
              <WikiSourceFiles
                files={shown.sourceFiles}
                owner={owner ?? toc?.owner ?? ""}
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
