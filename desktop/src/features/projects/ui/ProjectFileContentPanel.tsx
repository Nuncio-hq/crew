import { ChevronRight, FileDiff } from "lucide-react";
import * as React from "react";

import type { ProjectRepoFile } from "@/features/projects/hooks";
import { languageForPath } from "@/features/projects/lib/projectLanguages";
import {
  formatFileSize,
  formatLastChangedAt,
} from "@/features/projects/lib/projectsViewHelpers";
import { SyntaxHighlightedCode } from "@/shared/ui/markdown";
import { PROJECT_DETAIL_PANEL_CLASS } from "./projectPanelStyles";
import {
  type RepositoryFileContentSource,
  useRepositoryFileContent,
} from "./useRepositoryFileContent";

function BreadcrumbButton({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      className="truncate rounded-md px-1.5 py-1 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}

export function ProjectFileContentPanel({
  file,
  fileContentSource,
  highlightEnd,
  highlightStart,
  onOpenPath,
}: {
  file: ProjectRepoFile;
  fileContentSource?: RepositoryFileContentSource;
  highlightEnd?: number;
  highlightStart?: number;
  onOpenPath: (path: string) => void;
}) {
  const fileContent = useRepositoryFileContent(file, fileContentSource);
  const language = languageForPath(file.path);
  const pathSegments = file.path.split("/").filter(Boolean);
  const fileName = pathSegments[pathSegments.length - 1] ?? file.path;
  const directorySegments = pathSegments.slice(0, -1);

  React.useEffect(() => {
    if (highlightStart == null) return;
    const line = document.querySelector(`[data-line="${highlightStart}"]`);
    line?.scrollIntoView({ block: "center" });
  }, [highlightStart]);

  return (
    <div
      className={PROJECT_DETAIL_PANEL_CLASS}
      data-project-detail-panel
      data-testid="wiki-file-panel"
    >
      <div className="flex min-h-14 items-center gap-1 border-border/50 border-b bg-muted/20 px-3 py-3">
        <BreadcrumbButton onClick={() => onOpenPath("")}>
          Files
        </BreadcrumbButton>
        {directorySegments.map((segment, index) => {
          const nextPath = directorySegments.slice(0, index + 1).join("/");
          return (
            <React.Fragment key={nextPath}>
              <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground/60" />
              <BreadcrumbButton onClick={() => onOpenPath(nextPath)}>
                {segment}
              </BreadcrumbButton>
            </React.Fragment>
          );
        })}
        <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground/60" />
        <FileDiff className="h-4 w-4 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate px-1.5 py-1 font-mono text-xs text-foreground">
          {fileName}
        </span>
        <div className="hidden shrink-0 items-center gap-3 text-2xs text-muted-foreground sm:flex">
          <span>Last changed {formatLastChangedAt(file.lastChangedAt)}</span>
          <span>{formatFileSize(file.size)}</span>
        </div>
        <span className="shrink-0 text-2xs text-muted-foreground sm:hidden">
          {formatFileSize(file.size)}
        </span>
      </div>
      <div className="border-border/50 border-b bg-muted/10 px-4 py-2 text-2xs text-muted-foreground sm:hidden">
        Last changed {formatLastChangedAt(file.lastChangedAt)}
      </div>
      {fileContent.isLoading ? (
        <div className="p-6 text-sm text-muted-foreground">Loading file…</div>
      ) : fileContent.content ? (
        <pre className="max-h-[36rem] overflow-auto bg-background/60 p-4">
          {language ? (
            <SyntaxHighlightedCode
              className="text-xs leading-relaxed"
              code={fileContent.content}
              highlightEnd={highlightEnd}
              highlightStart={highlightStart}
              language={language}
            />
          ) : (
            <code className="block min-w-full whitespace-pre font-mono text-xs leading-relaxed text-foreground">
              {fileContent.content.split("\n").map((line, index) => {
                const lineNumber = index + 1;
                const highlighted =
                  highlightStart != null &&
                  highlightEnd != null &&
                  lineNumber >= highlightStart &&
                  lineNumber <= highlightEnd;
                return (
                  <span
                    className={highlighted ? "bg-attention/25" : undefined}
                    data-line={String(lineNumber)}
                    data-testid={
                      highlighted ? "wiki-file-highlight" : undefined
                    }
                    key={lineNumber}
                  >
                    {line}
                    {"\n"}
                  </span>
                );
              })}
            </code>
          )}
        </pre>
      ) : (
        <div className="p-6 text-sm text-muted-foreground">
          {fileContent.error
            ? "Could not load this file. Try again after refreshing the repository."
            : "Preview unavailable for this file. Large and binary files only show metadata."}
        </div>
      )}
    </div>
  );
}
