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

type SourceReference = [string, string, number, number, number];

function sourceReferences(event: RelayEvent): SourceReference[] {
  const tag = event.tags.find(
    (candidate) => candidate[0] === "wiki-source-files",
  );
  if (!tag?.[1]) return [];
  try {
    const value = JSON.parse(tag[1]) as unknown;
    if (!Array.isArray(value)) return [];
    return value.filter(
      (entry): entry is SourceReference =>
        Array.isArray(entry) &&
        entry.length === 5 &&
        entry.every(
          (part) => typeof part === "string" || typeof part === "number",
        ) &&
        typeof entry[0] === "string" &&
        typeof entry[1] === "string" &&
        typeof entry[2] === "number" &&
        typeof entry[3] === "number" &&
        typeof entry[4] === "number",
    );
  } catch {
    return [];
  }
}

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
}: {
  files: string[];
  owner: string;
  pageEvent: RelayEvent;
  repoD: string;
  operationScope?: OwnerOperationScope;
}) {
  const queryClient = useQueryClient();
  const coordinate = wikiRepositoryCoordinate(owner, repoD);
  const scopeKey = operationScope
    ? `${operationScope.scope.owner}:${operationScope.scope.community}:${operationScope.workspace_generation}:${operationScope.identity_generation}`
    : "unscoped";
  const grantsQueryKey = ["wiki-source-grants", coordinate, scopeKey] as const;
  const grantsQuery = useQuery({
    queryKey: grantsQueryKey,
    queryFn: async () => {
      try {
        return await listWikiSourceGrants();
      } catch {
        return [];
      }
    },
    enabled: Boolean(operationScope) && owner.length === 64 && repoD.length > 0,
    staleTime: 5_000,
  });
  const [preview, setPreview] = React.useState<WikiSourceContent | null>(null);
  const openRequestRef = React.useRef(0);
  const cancelPendingOpen = React.useCallback(() => {
    openRequestRef.current += 1;
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
  const references = sourceReferences(pageEvent);
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
    setPreview(null);
    resetMutationsRef.current();
  }, [sourceContextKey]);
  if (files.length === 0) return null;
  return (
    <details
      className="mb-4 rounded-md border border-border bg-muted/20 p-3"
      data-testid="wiki-source-files"
    >
      <summary className="cursor-pointer text-sm text-muted-foreground">
        Relevant source files ({files.length})
      </summary>
      <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
        {grant ? (
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
          return (
            <li key={file}>
              <button
                className="font-mono text-2xs text-primary"
                data-testid={`wiki-source-file-${file}`}
                disabled={!canOpen}
                onClick={(event) => {
                  event.preventDefault();
                  if (!grant || referenceIndex < 0) return;
                  const requestId = ++openRequestRef.current;
                  const requestContext = { ...sourceContextRef.current };
                  void openSource
                    .mutateAsync({
                      id: grant.capabilityId,
                      index: referenceIndex,
                    })
                    .then(
                      (result) => {
                        const current = sourceContextRef.current;
                        if (
                          requestId !== openRequestRef.current ||
                          current.pageId !== requestContext.pageId ||
                          current.coordinate !== requestContext.coordinate ||
                          current.grantId !== requestContext.grantId ||
                          (current.scope && requestContext.scope
                            ? !sameOwnerOperationScope(
                                current.scope,
                                requestContext.scope,
                              )
                            : current.scope !== requestContext.scope)
                        ) {
                          return;
                        }
                        setPreview(result);
                      },
                      () => undefined,
                    );
                }}
                type="button"
              >
                {file}
              </button>
            </li>
          );
        })}
      </ul>
      {!grant ? (
        <p
          className="mt-2 text-2xs text-muted-foreground"
          data-testid="wiki-source-unavailable"
        >
          {operationScope
            ? "Choose the linked folder to open cited source at its recorded revision."
            : "Source access is unavailable until the current Wiki scope is ready."}
        </p>
      ) : references.length === 0 || hasUnavailableReference ? (
        <p
          className="mt-2 text-2xs text-muted-foreground"
          data-testid="wiki-source-unavailable"
        >
          The cited source reference is unavailable for one or more files on
          this page.
        </p>
      ) : null}
      {preview ? (
        <pre
          className="mt-3 max-h-64 overflow-auto rounded border border-border bg-muted/20 p-3 text-xs"
          data-testid="wiki-source-preview"
        >
          {excerpt(preview)}
        </pre>
      ) : null}
    </details>
  );
}
