import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { useProjectsQuery } from "@/features/projects/hooks";
import type { Repository } from "@/features/projects/projectModels";
import {
  parseCompanyWikiPage,
  parseWikiPage,
  parseWikiToc,
  type CompanyWikiPage,
  type WikiPage,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import {
  captureOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";
import { relayClient } from "@/shared/api/relayClient";
import type { RelayEvent } from "@/shared/api/types";
import {
  assertWikiSnapshotScope,
  wikiRepositoryCoordinate,
  type WikiRepositoryReadStatus,
  type WikiSnapshotRead,
  type WikiSnapshotScopeKey,
} from "@/shared/api/wikiSnapshot";
import { activateWikiJobScope } from "@/features/wiki/lib/wikiStore";
import { KIND_LONG_FORM } from "@/shared/constants/kinds";
import {
  readWikiRepositoryProjection,
  registerWikiReadConsumer,
  retryWikiRepository,
  unregisterWikiReadConsumer,
} from "./wikiReadCoordinator";

export const wikiEventsQueryKey = ["crew-wiki-events"] as const;
const WIKI_SCOPE_QUERY_KEY = [...wikiEventsQueryKey, "scope"] as const;

export type WikiEventsProjection = {
  tocs: WikiToc[];
  pages: WikiPage[];
  company: CompanyWikiPage[];
  states: RelayEvent[];
  repositoryStatuses: Record<string, WikiRepositoryReadStatus>;
  companyError: Error | null;
};

export type WikiEventsQueryOptions = {
  /** Coordinates selected by the user and therefore read before automatic work. */
  priorityCoordinates?: readonly string[];
};

function uniqueRepositories(projects: { repositories: Repository[] }[]) {
  return [
    ...new Map(
      projects
        .flatMap((project) => project.repositories)
        .map((repository) => [repository.repoAddress, repository]),
    ).values(),
  ];
}

function repoProjectionQueryKey(
  scopeKey: WikiSnapshotScopeKey,
): readonly unknown[] {
  return [...wikiEventsQueryKey, "repos", ...scopeKey];
}

function companyQueryKey(scopeKey: WikiSnapshotScopeKey): readonly unknown[] {
  return [...wikiEventsQueryKey, "company", ...scopeKey];
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return typeof error === "string" ? error : "Wiki read failed.";
}

function flattenRepositoryProjection(
  projected: Map<
    string,
    {
      snapshot: WikiSnapshotRead | null;
      repoState: RelayEvent | null;
      repoStateFresh: boolean;
      status: WikiRepositoryReadStatus;
    }
  >,
): Omit<WikiEventsProjection, "company" | "companyError"> {
  const tocs: WikiToc[] = [];
  const pages: WikiPage[] = [];
  const states: RelayEvent[] = [];
  const repositoryStatuses: Record<string, WikiRepositoryReadStatus> = {};
  for (const [coordinate, value] of projected) {
    repositoryStatuses[coordinate] = value.status;
    if (value.repoStateFresh && value.repoState) states.push(value.repoState);
    const snapshot = value.snapshot;
    if (
      !snapshot ||
      (snapshot.state !== "complete" && snapshot.state !== "legacy")
    ) {
      continue;
    }
    if (snapshot.head) {
      const toc = parseWikiToc(snapshot.head);
      if (toc) {
        // A v1 cadence edit replaces only the head. Keep freshness anchored to
        // the immutable manifest generation time so changing a schedule does
        // not postpone the next generation window.
        tocs.push({
          ...toc,
          generatedAt:
            snapshot.state === "complete" && snapshot.manifest
              ? snapshot.manifest.created_at
              : toc.generatedAt,
        });
      }
    }
    for (const event of snapshot.pages) {
      const page = parseWikiPage(event);
      if (page) pages.push(page);
    }
  }
  return { tocs, pages, states, repositoryStatuses };
}

function tagValue(event: RelayEvent, name: string): string | undefined {
  return event.tags.find((tag) => tag[0] === name)?.[1];
}

function filterRepositoryProjection(
  projection: Omit<WikiEventsProjection, "company" | "companyError">,
  repositories: readonly Repository[],
): Omit<WikiEventsProjection, "company" | "companyError"> {
  const coordinates = new Set(
    repositories.map((repository) =>
      wikiRepositoryCoordinate(repository.owner, repository.dtag),
    ),
  );
  const states = projection.states.filter((event) => {
    const dtag = tagValue(event, "d");
    const association = tagValue(event, "a");
    return repositories.some(
      (repository) =>
        repository.dtag === dtag &&
        (event.pubkey.toLowerCase() === repository.owner.toLowerCase() ||
          association === repository.repoAddress),
    );
  });
  return {
    tocs: projection.tocs.filter((toc) =>
      coordinates.has(wikiRepositoryCoordinate(toc.owner, toc.repoD)),
    ),
    pages: projection.pages.filter((page) =>
      coordinates.has(wikiRepositoryCoordinate(page.event.pubkey, page.repoD)),
    ),
    states,
    repositoryStatuses: Object.fromEntries(
      Object.entries(projection.repositoryStatuses).filter(([coordinate]) =>
        coordinates.has(coordinate),
      ),
    ),
  };
}

// Control characters cannot appear in a repository coordinate, so they make a
// signature that is unambiguous for both joining and splitting.
const COORDINATE_SEPARATOR = String.fromCharCode(0);
const FIELD_SEPARATOR = String.fromCharCode(1);

function coordinateSignatureOf(coordinates: readonly string[]): string {
  return coordinates.join(COORDINATE_SEPARATOR);
}

function coordinatesFrom(signature: string): string[] {
  return signature ? signature.split(COORDINATE_SEPARATOR) : [];
}

function repositoryFilterSignatureOf(
  repositories: readonly Repository[] | undefined,
): string | null {
  if (!repositories) return null;
  return repositories
    .map((repository) =>
      [
        wikiRepositoryCoordinate(repository.owner, repository.dtag),
        repository.repoAddress,
      ].join(FIELD_SEPARATOR),
    )
    .join(COORDINATE_SEPARATOR);
}

/**
 * Hold one value identity per distinct signature.
 *
 * Callers build the repository filter inline, so its identity changes on every
 * render while its meaning does not. Deriving anything from that identity
 * would rebuild the coordinate arrays on each render and retire, re-register
 * and refetch the shared canonical read for renders that changed nothing.
 */
function useStableBySignature<T>(value: T, signature: string | null): T {
  const held = React.useRef<{ signature: string | null; value: T }>({
    signature,
    value,
  });
  if (held.current.signature !== signature) {
    held.current = { signature, value };
  }
  return held.current.value;
}

function scopeKeyFor(
  scope: OwnerOperationScope | undefined,
): WikiSnapshotScopeKey {
  return scope
    ? [
        scope.scope.owner,
        scope.scope.community,
        scope.workspace_generation,
        scope.identity_generation,
      ]
    : ["", "", -1, -1];
}

/**
 * Read repository Wikis through one canonical projection per active scope.
 * Every mounted consumer contributes its repository manifest to the shared
 * coordinator; filtered project views are derived from that full projection.
 */
export function useWikiEventsQuery(
  repositoryFilter?: readonly Repository[],
  options: WikiEventsQueryOptions = {},
) {
  const queryClient = useQueryClient();
  const consumerId = React.useId();
  // All consumers observe the same full project collection. A filtered tab
  // contributes its exact repository as a priority even while that collection
  // is still loading, so it cannot disappear from the canonical manifest.
  const projectsQuery = useProjectsQuery(true);
  // Every value derived below is keyed on a canonical signature rather than on
  // the caller's array identity, so equivalent re-renders neither rebuild the
  // projection nor disturb the shared consumer registration.
  const filterRepositories = useStableBySignature(
    repositoryFilter,
    repositoryFilterSignatureOf(repositoryFilter),
  );
  const repositories = React.useMemo(() => {
    const byAddress = new Map(
      uniqueRepositories(projectsQuery.data ?? []).map((repository) => [
        repository.repoAddress,
        repository,
      ]),
    );
    for (const repository of filterRepositories ?? []) {
      byAddress.set(repository.repoAddress, repository);
    }
    return [...byAddress.values()];
  }, [projectsQuery.data, filterRepositories]);
  const coordinateSignature = coordinateSignatureOf([
    ...new Set(
      repositories.map((repository) =>
        wikiRepositoryCoordinate(repository.owner, repository.dtag),
      ),
    ),
  ]);
  const coordinates = React.useMemo(
    () => coordinatesFrom(coordinateSignature),
    [coordinateSignature],
  );
  const prioritySignature = coordinateSignatureOf(
    [
      ...new Set([
        ...(filterRepositories ?? []).map((repository) =>
          wikiRepositoryCoordinate(repository.owner, repository.dtag),
        ),
        ...(options.priorityCoordinates ?? []),
      ]),
    ].filter((coordinate) => coordinates.includes(coordinate)),
  );
  const priorityCoordinates = React.useMemo(
    () => coordinatesFrom(prioritySignature),
    [prioritySignature],
  );

  const scopeQuery = useQuery({
    queryKey: WIKI_SCOPE_QUERY_KEY,
    queryFn: async () => {
      const next = await captureOwnerOperationScope();
      if (!activateWikiJobScope(next)) {
        throw new Error(
          "The captured owner or workspace scope is older than the active Wiki scope.",
        );
      }
      return next;
    },
    retry: false,
    staleTime: 0,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: true,
  });
  // React Query retains successful data through a refetch error. That is
  // useful for ordinary lists but unsafe for owner-scoped Wiki content: a
  // failed scope capture revokes readiness until a new token is captured.
  // Its structural sharing keeps this reference stable while the poll returns
  // an equal token, which is what keeps the derived key and effect stable.
  const scope = scopeQuery.error ? undefined : scopeQuery.data;
  const scopeKey = React.useMemo(() => scopeKeyFor(scope), [scope]);
  const repoQueryKey = React.useMemo(
    () => repoProjectionQueryKey(scopeKey),
    [scopeKey],
  );

  React.useEffect(() => {
    if (!scope) return;
    registerWikiReadConsumer(
      scope,
      consumerId,
      coordinates,
      priorityCoordinates,
    );
    const queryState = queryClient.getQueryState(repoQueryKey);
    if (queryState?.fetchStatus !== "fetching") {
      void queryClient.invalidateQueries(
        { queryKey: repoQueryKey, exact: true },
        { cancelRefetch: false },
      );
    }
    return () => unregisterWikiReadConsumer(scope, consumerId);
  }, [
    consumerId,
    coordinates,
    priorityCoordinates,
    queryClient,
    repoQueryKey,
    scope,
  ]);

  React.useEffect(() => {
    if (!scopeQuery.error) return;
    void queryClient.cancelQueries({
      queryKey: [...wikiEventsQueryKey, "repos"],
    });
  }, [queryClient, scopeQuery.error]);

  const repoQuery = useQuery({
    queryKey: repoQueryKey,
    enabled: scope !== undefined,
    queryFn: async ({ signal }) => {
      if (!scope) throw new Error("Wiki scope is not ready.");
      const projected = await readWikiRepositoryProjection(
        scope,
        consumerId,
        coordinates,
        priorityCoordinates,
        signal,
      );
      return flattenRepositoryProjection(projected);
    },
    retry: false,
    staleTime: 10_000,
    // The projection contains the complete flattened page graph. Retain it
    // only while a scoped consumer is mounted; the coordinator retires the
    // matching scope on the last consumer and aborts any in-flight read.
    gcTime: 0,
  });

  const companyQuery = useQuery({
    queryKey: companyQueryKey(scopeKey),
    enabled: scope !== undefined,
    queryFn: async ({ signal }) => {
      if (!scope) throw new Error("Wiki scope is not ready.");
      const events = await relayClient.fetchEvents({
        kinds: [KIND_LONG_FORM],
        limit: 200,
      });
      await assertWikiSnapshotScope(scope, signal);
      return events
        .map(parseCompanyWikiPage)
        .filter((page): page is CompanyWikiPage => page !== null);
    },
    retry: false,
    staleTime: 10_000,
    // Company pages are scoped to the same owner-operation token and can be
    // large as well. Do not leave an inactive scope copy in QueryClient.
    gcTime: 0,
  });

  const data = React.useMemo<WikiEventsProjection | undefined>(() => {
    if (!scope) return undefined;
    const fullRepositoryData = repoQuery.data ?? {
      tocs: [],
      pages: [],
      states: [],
      repositoryStatuses: {},
    };
    const repositoryData =
      filterRepositories === undefined
        ? fullRepositoryData
        : filterRepositoryProjection(fullRepositoryData, filterRepositories);
    return {
      ...repositoryData,
      company: companyQuery.data ?? [],
      companyError:
        companyQuery.error instanceof Error
          ? companyQuery.error
          : companyQuery.error
            ? new Error(errorMessage(companyQuery.error))
            : null,
    };
  }, [
    companyQuery.data,
    companyQuery.error,
    filterRepositories,
    repoQuery.data,
    scope,
  ]);

  const retryRepository = React.useCallback(
    (coordinate: string) => {
      if (!scope || !coordinates.includes(coordinate)) return;
      retryWikiRepository(scope, coordinate);
      const queryState = queryClient.getQueryState(repoQueryKey);
      if (queryState?.fetchStatus !== "fetching") {
        void queryClient.invalidateQueries(
          { queryKey: repoQueryKey, exact: true },
          { cancelRefetch: false },
        );
      }
    },
    [coordinates, queryClient, repoQueryKey, scope],
  );

  const scopeError = scopeQuery.error
    ? new Error(errorMessage(scopeQuery.error))
    : null;
  return {
    ...repoQuery,
    data,
    error:
      scopeError ??
      repoQuery.error ??
      (companyQuery.error ? new Error(errorMessage(companyQuery.error)) : null),
    isError: Boolean(scopeError) || repoQuery.isError || companyQuery.isError,
    isFetching:
      scopeQuery.isFetching ||
      (scope !== undefined &&
        (repoQuery.isFetching || companyQuery.isFetching)),
    isPending:
      scopeQuery.isPending ||
      (scope !== undefined && (repoQuery.isPending || companyQuery.isPending)),
    companyQuery,
    scopeQuery,
    retryRepository,
  };
}
