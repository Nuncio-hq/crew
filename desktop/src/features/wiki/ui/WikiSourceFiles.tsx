import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  sameOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";
import type { RelayEvent } from "@/shared/api/types";
import { wikiRepositoryCoordinate } from "@/shared/api/wikiSnapshot";
import {
  chooseWikiSourceRoot,
  forgetWikiSourceRoot,
  listWikiSourceGrants,
  openWikiSource,
  type WikiSourceContent,
} from "@/shared/api/wikiSource";
import {
  findWikiSourceReference,
  wikiSourceReferences,
  type WikiSourceOpenRequest,
} from "@/features/wiki/lib/wikiSourceReferences";

export type { WikiSourceOpenRequest } from "@/features/wiki/lib/wikiSourceReferences";

type WikiSourceFilesProps = {
  files: string[];
  owner: string;
  pageEvent: RelayEvent;
  repoD: string;
  operationScope?: OwnerOperationScope;
  /** Inline is retained for the article summary; pane is the verified reader. */
  mode?: "inline" | "pane";
  /** Exact reference to open automatically when the pane receives focus. */
  initialRequest?: WikiSourceOpenRequest | null;
  /** Open the page-level source pane from an article control. */
  onOpenPane?: (
    request: WikiSourceOpenRequest,
    trigger?: HTMLElement | null,
  ) => void;
  /** Dismiss the page-level source pane. */
  onClosePane?: () => void;
};

function excerpt(source: WikiSourceContent): string {
  const lines = source.content.split("\n");
  return lines
    .slice(Math.max(0, source.startLine - 1), source.endLine)
    .join("\n");
}

export function WikiSourceFiles({
  files,
  owner,
  pageEvent,
  repoD,
  operationScope,
  mode = "inline",
  initialRequest,
  onOpenPane,
  onClosePane,
}: WikiSourceFilesProps) {
  const queryClient = useQueryClient();
  const coordinate = wikiRepositoryCoordinate(owner, repoD);
  const scopeKey = operationScope
    ? `${operationScope.scope.owner}:${operationScope.scope.community}:${operationScope.workspace_generation}:${operationScope.identity_generation}`
    : "unscoped";
  const grantsQueryKey = ["wiki-source-grants", coordinate, scopeKey] as const;
  const grantsQuery = useQuery({
    queryKey: grantsQueryKey,
    queryFn: () => listWikiSourceGrants(),
    enabled: Boolean(operationScope) && owner.length === 64 && repoD.length > 0,
    retry: false,
    staleTime: 5_000,
  });
  const [preview, setPreview] = React.useState<WikiSourceContent | null>(null);
  const openRequestRef = React.useRef(0);
  const paneRequestRef = React.useRef<string | null>(null);
  const cancelPendingOpen = React.useCallback(() => {
    openRequestRef.current += 1;
    paneRequestRef.current = null;
    setPreview(null);
  }, []);
  const choose = useMutation({
    mutationFn: () => chooseWikiSourceRoot(coordinate),
    onMutate: cancelPendingOpen,
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: grantsQueryKey,
      });
    },
  });
  const forget = useMutation({
    mutationFn: (capabilityId: string) => forgetWikiSourceRoot(capabilityId),
    onMutate: cancelPendingOpen,
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: grantsQueryKey,
      });
    },
  });
  const openSource = useMutation({
    mutationFn: ({ id, index }: { id: string; index: number }) =>
      openWikiSource(id, pageEvent, index),
  });
  const resetMutationsRef = React.useRef<() => void>(() => undefined);
  resetMutationsRef.current = () => {
    openSource.reset();
    choose.reset();
    forget.reset();
  };
  const grant = grantsQuery.data?.find(
    (candidate) =>
      candidate.repositoryCoordinate === coordinate &&
      operationScope !== undefined &&
      sameOwnerOperationScope(candidate.token, operationScope),
  );
  const references = React.useMemo(
    () => wikiSourceReferences(pageEvent),
    [pageEvent],
  );
  const hasUnavailableReference = files.some(
    (file) => references.findIndex((reference) => reference[0] === file) < 0,
  );
  const sourceContextRef = React.useRef<{
    pageId: string;
    coordinate: string;
    scope: OwnerOperationScope | undefined;
    grantId: string | null;
  }>({
    pageId: pageEvent.id,
    coordinate,
    scope: operationScope,
    grantId: grant?.capabilityId ?? null,
  });
  sourceContextRef.current = {
    pageId: pageEvent.id,
    coordinate,
    scope: operationScope,
    grantId: grant?.capabilityId ?? null,
  };
  const sourceContextKey = JSON.stringify([
    coordinate,
    grant?.capabilityId ?? null,
    pageEvent.id,
    scopeKey,
  ]);
  React.useEffect(() => {
    if (!sourceContextKey) return;
    openRequestRef.current += 1;
    paneRequestRef.current = null;
    setPreview(null);
    resetMutationsRef.current();
  }, [sourceContextKey]);

  const openReference = React.useCallback(
    (referenceIndex: number) => {
      if (!grant) return;
      const requestId = ++openRequestRef.current;
      const requestContext = { ...sourceContextRef.current };
      void openSource
        .mutateAsync({ id: grant.capabilityId, index: referenceIndex })
        .then(
          (result) => {
            const current = sourceContextRef.current;
            if (
              requestId !== openRequestRef.current ||
              current.pageId !== requestContext.pageId ||
              current.coordinate !== requestContext.coordinate ||
              current.grantId !== requestContext.grantId ||
              (current.scope && requestContext.scope
                ? !sameOwnerOperationScope(current.scope, requestContext.scope)
                : current.scope !== requestContext.scope)
            ) {
              return;
            }
            setPreview(result);
          },
          () => undefined,
        );
    },
    [grant, openSource],
  );

  const initialRequestKey = initialRequest
    ? JSON.stringify(initialRequest)
    : null;
  React.useEffect(() => {
    if (
      mode !== "pane" ||
      !grant ||
      !initialRequest ||
      choose.isPending ||
      forget.isPending
    )
      return;
    const match = findWikiSourceReference(pageEvent, initialRequest);
    if (!match) return;
    const requestKey = `${sourceContextKey}:${initialRequestKey}`;
    if (paneRequestRef.current === requestKey) return;
    paneRequestRef.current = requestKey;
    openReference(match.index);
  }, [
    grant,
    choose.isPending,
    forget.isPending,
    initialRequest,
    initialRequestKey,
    mode,
    openReference,
    pageEvent,
    sourceContextKey,
  ]);

  if (files.length === 0) return null;

  const contents = (
    <>
      <div className="flex flex-wrap items-center gap-2 text-xs">
        {grantsQuery.error ? (
          <>
            <span
              className="text-destructive"
              data-testid="wiki-source-grants-error"
            >
              Source access could not be checked.
            </span>
            <button
              className="rounded border border-border px-2 py-1 text-muted-foreground hover:text-foreground"
              data-testid="wiki-source-grants-retry"
              onClick={() => void grantsQuery.refetch()}
              type="button"
            >
              Retry source access
            </button>
          </>
        ) : grant ? (
          <>
            <span className="text-muted-foreground">Folder: {grant.label}</span>
            <button
              className="rounded border border-border px-2 py-1 text-muted-foreground hover:text-foreground"
              disabled={forget.isPending}
              onClick={(event) => {
                event.preventDefault();
                void forget.mutateAsync(grant.capabilityId);
              }}
              type="button"
            >
              Forget folder
            </button>
          </>
        ) : (
          <button
            className="rounded border border-border px-2 py-1 text-primary hover:text-foreground"
            disabled={choose.isPending || !operationScope}
            onClick={(event) => {
              event.preventDefault();
              void choose.mutateAsync();
            }}
            type="button"
          >
            {choose.isPending
              ? "Waiting for folder…"
              : "Choose folder to open source"}
          </button>
        )}
        {choose.error ? (
          <span className="text-destructive">{choose.error.message}</span>
        ) : null}
        {forget.error ? (
          <span className="text-destructive">{forget.error.message}</span>
        ) : null}
        {openSource.isPending ? (
          <span
            className="text-muted-foreground"
            data-testid="wiki-source-loading"
          >
            Opening verified source…
          </span>
        ) : null}
        {openSource.error ? (
          <span className="text-destructive">{openSource.error.message}</span>
        ) : null}
      </div>
      <ul className="mt-2 space-y-1">
        {files.map((file) => {
          const referenceIndex = references.findIndex(
            (reference) => reference[0] === file,
          );
          const canOpen = Boolean(grant && referenceIndex >= 0);
          const request =
            referenceIndex >= 0
              ? {
                  path: file,
                  startLine: references[referenceIndex]?.[3],
                  endLine: references[referenceIndex]?.[4],
                }
              : { path: file };
          return (
            <li key={file}>
              <button
                className="font-mono text-2xs text-primary"
                data-testid={`wiki-source-file-${file}`}
                disabled={!canOpen}
                onClick={(event) => {
                  event.preventDefault();
                  if (!grant || referenceIndex < 0) return;
                  if (mode === "inline" && onOpenPane) {
                    onOpenPane(request, event.currentTarget);
                    return;
                  }
                  openReference(referenceIndex);
                }}
                type="button"
              >
                {file}
              </button>
            </li>
          );
        })}
      </ul>
      {!grant && !grantsQuery.error ? (
        <p
          className="mt-2 text-2xs text-muted-foreground"
          data-testid="wiki-source-unavailable"
        >
          {operationScope
            ? "Choose the linked folder to open cited source at its recorded revision."
            : "Source access is unavailable until the current Wiki scope is ready."}
        </p>
      ) : !grantsQuery.error &&
        (references.length === 0 || hasUnavailableReference) ? (
        <p
          className="mt-2 text-2xs text-muted-foreground"
          data-testid="wiki-source-unavailable"
        >
          The cited source reference is unavailable for one or more files on
          this page.
        </p>
      ) : null}
      {mode === "pane" &&
      initialRequest &&
      !findWikiSourceReference(pageEvent, initialRequest) ? (
        <p
          className="mt-2 text-2xs text-muted-foreground"
          data-testid="wiki-source-unavailable"
        >
          This source citation is unavailable for the published page revision.
        </p>
      ) : null}
      {preview ? (
        <pre
          className="mt-3 max-h-[min(65vh,36rem)] overflow-auto rounded border border-border bg-muted/20 p-3 text-xs"
          data-testid="wiki-source-preview"
        >
          {excerpt(preview)}
        </pre>
      ) : null}
    </>
  );

  if (mode === "pane") {
    return (
      <section
        aria-label="Verified source"
        className="rounded-md border border-border bg-muted/20 p-3"
        data-testid="wiki-source-pane"
      >
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-sm font-semibold">Verified source</h2>
          {onClosePane ? (
            <button
              aria-label="Close source view"
              className="rounded border border-border px-2 py-1 text-2xs text-muted-foreground hover:text-foreground"
              data-testid="wiki-source-pane-close"
              onClick={onClosePane}
              type="button"
            >
              Close
            </button>
          ) : null}
        </div>
        <p className="mt-1 text-2xs text-muted-foreground">
          Read the exact source revision cited by this Wiki page.
        </p>
        <div className="mt-3">{contents}</div>
      </section>
    );
  }

  return (
    <details
      className="mb-4 rounded-md border border-border bg-muted/20 p-3"
      data-testid="wiki-source-files"
    >
      <summary className="cursor-pointer text-sm text-muted-foreground">
        Relevant source files ({files.length})
      </summary>
      <div className="mt-2">{contents}</div>
    </details>
  );
}
