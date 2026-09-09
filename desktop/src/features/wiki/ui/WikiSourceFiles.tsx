import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { buildFileLink } from "@/shared/lib/entityLink";
import { useOpenEntityLink } from "@/shared/ui/markdown/entityLinks";
import { parseEntityLink } from "@/shared/lib/entityLink";
import type { RelayEvent } from "@/shared/api/types";
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
}: {
  files: string[];
  owner: string;
  pageEvent: RelayEvent;
  repoD: string;
}) {
  const open = useOpenEntityLink();
  const queryClient = useQueryClient();
  const coordinate = `30617:${owner}:${repoD}`;
  const grantsQuery = useQuery({
    queryKey: ["wiki-source-grants", coordinate],
    queryFn: async () => {
      try {
        return await listWikiSourceGrants();
      } catch {
        return [];
      }
    },
    enabled: owner.length === 64 && repoD.length > 0,
    staleTime: 5_000,
  });
  const choose = useMutation({
    mutationFn: () => chooseWikiSourceRoot(coordinate),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["wiki-source-grants", coordinate],
      });
    },
  });
  const forget = useMutation({
    mutationFn: (capabilityId: string) => forgetWikiSourceRoot(capabilityId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["wiki-source-grants", coordinate],
      });
    },
  });
  const openSource = useMutation({
    mutationFn: ({ id, index }: { id: string; index: number }) =>
      openWikiSource(id, pageEvent, index),
  });
  const [preview, setPreview] = React.useState<WikiSourceContent | null>(null);
  React.useEffect(() => {
    if (!pageEvent.id) return;
    setPreview(null);
  }, [pageEvent.id]);
  if (files.length === 0) return null;
  const grant = grantsQuery.data?.find(
    (candidate) => candidate.repositoryCoordinate === coordinate,
  );
  const references = sourceReferences(pageEvent);
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
            disabled={choose.isPending}
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
          const href =
            owner.length === 64
              ? buildFileLink({
                  owner,
                  dtag: repoD,
                  path: file,
                  lines: "1-40",
                })
              : null;
          return (
            <li key={file}>
              <button
                className="font-mono text-2xs text-primary"
                onClick={(event) => {
                  event.preventDefault();
                  const referenceIndex = references.findIndex(
                    (reference) => reference[0] === file,
                  );
                  if (grant && referenceIndex >= 0) {
                    void openSource
                      .mutateAsync({
                        id: grant.capabilityId,
                        index: referenceIndex,
                      })
                      .then(setPreview);
                    return;
                  }
                  if (!href) return;
                  const parsed = parseEntityLink(href);
                  if (parsed.ok) open(parsed.value);
                }}
                type="button"
              >
                {file}
              </button>
            </li>
          );
        })}
      </ul>
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
