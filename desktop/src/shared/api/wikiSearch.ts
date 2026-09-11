import * as React from "react";
import { useQuery } from "@tanstack/react-query";

import type { WikiPage } from "@/features/wiki/lib/wikiEvents";
import { parseWikiDTag, parseWikiPage } from "@/features/wiki/lib/wikiEvents";
import {
  assertWikiSnapshotScope,
  wikiRepositoryCoordinate,
  wikiSnapshotScopeKey,
  type WikiSnapshotRead,
} from "@/shared/api/wikiSnapshot";
import { invokeTauri } from "@/shared/api/tauri";
import {
  sameOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_REPO_WIKI_PAGE } from "@/shared/constants/kinds";

export const MAX_WIKI_SEARCH_QUERY_CHARS = 256;
export const MAX_WIKI_SEARCH_RESULTS = 50;
export const WIKI_SEARCH_DEBOUNCE_MS = 150;
export const MAX_ACTIVE_WIKI_SEARCHES = 1;
const MAX_WIKI_SEARCH_EXCERPT_CHARS = 240;

type RawWikiSearchRead = {
  events: RelayEvent[];
  truncated: boolean;
};

export type WikiSearchResult = {
  pageEventId: string;
  snapshotId: string;
  sourceRevision: string;
  slug: string;
  title: string;
  section: string;
  match: string;
  excerpt: string;
  page: WikiPage;
};

export type WikiSearchResponse = {
  snapshotId: string;
  sourceRevision: string;
  results: WikiSearchResult[];
  truncated: boolean;
};

type SnapshotIdentity = {
  snapshotId: string;
  sourceRevision: string;
  coordinate: string;
};

type NativeWikiSearchArgs = {
  expected: OwnerOperationScope;
  coordinate: string;
  snapshotId: string;
  pageIds: string[];
  query: string;
};

type NativeWikiSearchRequest = {
  args: NativeWikiSearchArgs;
  signal?: AbortSignal;
  started: boolean;
  cancelled: boolean;
  settled: boolean;
  resolve: (value: {
    token: OwnerOperationScope;
    value: RawWikiSearchRead;
  }) => void;
  reject: (reason: unknown) => void;
  removeAbortListener: () => void;
};

let activeNativeWikiSearch: NativeWikiSearchRequest | null = null;
let queuedNativeWikiSearch: NativeWikiSearchRequest | null = null;

function wikiSearchAbortError(): Error {
  const error = new Error("Wiki search was cancelled.");
  error.name = "AbortError";
  return error;
}

/**
 * Keep the native search path single-flight and retain only the newest queued
 * request. Tauri invoke currently ignores AbortSignal, so aborting a query
 * cannot stop native IO; this admission gate still bounds native work to one
 * active request plus one latest pending request.
 */
function invokeWikiSearchBounded(
  args: NativeWikiSearchArgs,
  signal?: AbortSignal,
): Promise<{ token: OwnerOperationScope; value: RawWikiSearchRead }> {
  if (signal?.aborted) return Promise.reject(wikiSearchAbortError());
  return new Promise((resolve, reject) => {
    const request: NativeWikiSearchRequest = {
      args,
      signal,
      started: false,
      cancelled: false,
      settled: false,
      resolve,
      reject,
      removeAbortListener: () => {},
    };
    const rejectCancelled = () => {
      request.cancelled = true;
      if (!request.settled) {
        request.settled = true;
        reject(wikiSearchAbortError());
      }
      if (!request.started && queuedNativeWikiSearch === request) {
        queuedNativeWikiSearch = null;
      }
    };
    if (signal) {
      signal.addEventListener("abort", rejectCancelled, { once: true });
      request.removeAbortListener = () =>
        signal.removeEventListener("abort", rejectCancelled);
    }

    if (!activeNativeWikiSearch) {
      activeNativeWikiSearch = request;
      request.started = true;
      void runNativeWikiSearch(request);
      return;
    }

    if (queuedNativeWikiSearch) {
      queuedNativeWikiSearch.cancelled = true;
      if (!queuedNativeWikiSearch.settled) {
        queuedNativeWikiSearch.settled = true;
        queuedNativeWikiSearch.reject(wikiSearchAbortError());
      }
      queuedNativeWikiSearch.removeAbortListener();
    }
    queuedNativeWikiSearch = request;
  });
}

async function runNativeWikiSearch(
  request: NativeWikiSearchRequest,
): Promise<void> {
  try {
    const result = await invokeTauri<{
      token: OwnerOperationScope;
      value: RawWikiSearchRead;
    }>("wiki_search", request.args);
    if (!request.cancelled && !request.signal?.aborted && !request.settled) {
      request.settled = true;
      request.resolve(result);
    }
  } catch (error) {
    if (!request.cancelled && !request.settled) {
      request.settled = true;
      request.reject(error);
    }
  } finally {
    request.removeAbortListener();
    if (activeNativeWikiSearch === request) activeNativeWikiSearch = null;
    const next = queuedNativeWikiSearch;
    queuedNativeWikiSearch = null;
    if (next && !next.cancelled && !next.signal?.aborted && !next.settled) {
      activeNativeWikiSearch = next;
      next.started = true;
      void runNativeWikiSearch(next);
    } else if (next && !next.settled) {
      next.cancelled = true;
      next.settled = true;
      next.removeAbortListener();
      next.reject(wikiSearchAbortError());
    }
  }
}

function useDebouncedValue(value: string, delayMs: number): string {
  const [debounced, setDebounced] = React.useState(value);
  React.useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [delayMs, value]);
  return debounced;
}

/** Normalize and bound one user-entered body-search query. */
export function normalizeWikiSearchQuery(value: string): string {
  const query = value.trim();
  if (query.length === 0) return "";
  if ([...query].length > MAX_WIKI_SEARCH_QUERY_CHARS) {
    throw new Error(
      `Wiki search query must contain at most ${MAX_WIKI_SEARCH_QUERY_CHARS} characters.`,
    );
  }
  return query;
}

/** Return the exact body match used to build a truthful result excerpt. */
export function findWikiBodyMatch(
  content: string,
  query: string,
): { index: number; text: string } | null {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return null;
  const lower = content.toLocaleLowerCase();
  const direct = lower.indexOf(normalized);
  if (direct >= 0) {
    return {
      index: direct,
      text: content.slice(direct, direct + normalized.length),
    };
  }
  const tokens = normalized.split(/\s+/u).filter(Boolean);
  for (const token of tokens) {
    const index = lower.indexOf(token);
    if (index >= 0) {
      return { index, text: content.slice(index, index + token.length) };
    }
  }
  return null;
}

/** Build a bounded excerpt around the first body match. */
export function wikiSearchExcerpt(
  content: string,
  match: { index: number; text: string },
  maxChars = MAX_WIKI_SEARCH_EXCERPT_CHARS,
): string {
  if (content.length <= maxChars) return content;
  const half = Math.max(1, Math.floor((maxChars - match.text.length) / 2));
  let start = Math.max(0, match.index - half);
  let end = Math.min(content.length, start + maxChars);
  if (end - start < maxChars) start = Math.max(0, end - maxChars);
  const prefix = start > 0 ? "…" : "";
  const suffix = end < content.length ? "…" : "";
  const available = maxChars - prefix.length - suffix.length;
  if (end - start > available) {
    end = start + available;
  }
  return `${prefix}${content.slice(start, end)}${suffix}`;
}

function tagValues(event: RelayEvent, name: string): string[][] {
  return event.tags.filter((tag) => tag[0] === name);
}

function exactTag(event: RelayEvent, name: string): string | null {
  const tags = tagValues(event, name);
  return tags.length === 1 && tags[0]?.length === 2
    ? (tags[0][1] ?? null)
    : null;
}

function validSnapshotId(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u.test(
    value,
  );
}

function validEventId(value: string): boolean {
  return /^[0-9a-f]{64}$/u.test(value);
}

function sourceRevisionFromManifest(manifest: RelayEvent): string | null {
  try {
    const value: unknown = JSON.parse(manifest.content);
    return Array.isArray(value) && typeof value[4] === "string" && value[4]
      ? value[4]
      : null;
  } catch {
    return null;
  }
}

/**
 * Extract the identity that the native snapshot verifier already authenticated.
 * Returning null makes legacy, missing, and incomplete graphs unavailable to
 * remote body search rather than mixing generations or falling back to a live
 * unscoped query.
 */
export function verifiedWikiSnapshotIdentity(
  snapshot: WikiSnapshotRead | null | undefined,
  coordinate: string,
): SnapshotIdentity | null {
  if (snapshot?.state !== "complete" || !snapshot.head || !snapshot.manifest) {
    return null;
  }
  const owner = snapshot.head.pubkey.toLowerCase();
  const headCoordinate = exactTag(snapshot.head, "a");
  const manifestCoordinate = exactTag(snapshot.manifest, "a");
  const headSnapshot = exactTag(snapshot.head, "wiki-snapshot");
  const manifestSnapshot = exactTag(snapshot.manifest, "wiki-snapshot");
  const headD = exactTag(snapshot.head, "d");
  const parsed = parseWikiDTag(headD ?? undefined);
  const expectedCoordinate = parsed
    ? wikiRepositoryCoordinate(owner, parsed.repoD)
    : null;
  if (
    !expectedCoordinate ||
    coordinate !== expectedCoordinate ||
    headCoordinate !== coordinate ||
    manifestCoordinate !== coordinate ||
    !headSnapshot ||
    headSnapshot !== manifestSnapshot ||
    !validSnapshotId(headSnapshot)
  ) {
    return null;
  }
  const sourceRevision = sourceRevisionFromManifest(snapshot.manifest);
  if (!sourceRevision) return null;
  return {
    snapshotId: headSnapshot,
    sourceRevision,
    coordinate,
  };
}

function validSearchEvent(
  event: RelayEvent,
  identity: SnapshotIdentity,
): boolean {
  if (!validEventId(event.id) || event.kind !== KIND_REPO_WIKI_PAGE)
    return false;
  if (event.pubkey.toLowerCase() !== identity.coordinate.split(":")[1]) {
    return false;
  }
  if (exactTag(event, "a") !== identity.coordinate) return false;
  if (exactTag(event, "wiki-version") !== "1") return false;
  if (exactTag(event, "wiki-snapshot") !== identity.snapshotId) return false;
  const d = exactTag(event, "d");
  if (!d) return false;
  const repoD = identity.coordinate.split(":").slice(2).join(":");
  const parsed = parseWikiDTag(d);
  return Boolean(parsed && parsed.repoD === repoD && parsed.slug !== "_toc");
}

/**
 * Execute one scoped NIP-50 search and project hits onto the already verified
 * immutable page graph. Remote event bodies are never rendered directly.
 */
export async function searchWikiAtScope(input: {
  scope: OwnerOperationScope;
  coordinate: string;
  snapshot: WikiSnapshotRead;
  query: string;
  signal?: AbortSignal;
}): Promise<WikiSearchResponse> {
  const query = normalizeWikiSearchQuery(input.query);
  const identity = verifiedWikiSnapshotIdentity(
    input.snapshot,
    input.coordinate,
  );
  if (!identity) {
    throw new Error("Wiki body search needs a complete verified snapshot.");
  }
  if (!query) {
    return {
      snapshotId: identity.snapshotId,
      sourceRevision: identity.sourceRevision,
      results: [],
      truncated: false,
    };
  }
  const pageIds = input.snapshot.pages
    .filter((event) => validSearchEvent(event, identity))
    .map((event) => event.id);
  if (pageIds.length === 0) {
    return {
      snapshotId: identity.snapshotId,
      sourceRevision: identity.sourceRevision,
      results: [],
      truncated: false,
    };
  }
  const raw = await invokeWikiSearchBounded(
    {
      expected: input.scope,
      coordinate: input.coordinate,
      snapshotId: identity.snapshotId,
      pageIds,
      query,
    },
    input.signal,
  );
  if (!sameOwnerOperationScope(raw.token, input.scope)) {
    throw new Error(
      "The active owner or workspace scope changed during Wiki search.",
    );
  }
  await assertWikiSnapshotScope(input.scope, input.signal);
  if (
    !Array.isArray(raw.value.events) ||
    typeof raw.value.truncated !== "boolean" ||
    raw.value.events.length > MAX_WIKI_SEARCH_RESULTS + 1 ||
    (raw.value.events.length > MAX_WIKI_SEARCH_RESULTS && !raw.value.truncated)
  ) {
    throw new Error("Wiki search returned an unbounded result.");
  }
  const pages = new Map(
    input.snapshot.pages
      .map((event) => [event.id, parseWikiPage(event)] as const)
      .filter((entry): entry is readonly [string, WikiPage] =>
        Boolean(entry[1]),
      ),
  );
  const seen = new Set<string>();
  const results: WikiSearchResult[] = [];
  for (const event of raw.value.events) {
    if (!validSearchEvent(event, identity) || seen.has(event.id)) {
      throw new Error(
        "Wiki search returned an event outside the verified snapshot.",
      );
    }
    const page = pages.get(event.id);
    if (!page) {
      throw new Error(
        "Wiki search returned an event from an old Wiki generation.",
      );
    }
    seen.add(event.id);
    const match = findWikiBodyMatch(page.content, query);
    if (!match) continue;
    results.push({
      pageEventId: page.event.id,
      snapshotId: identity.snapshotId,
      sourceRevision: identity.sourceRevision,
      slug: page.slug,
      title: page.title,
      section: page.section,
      match: match.text,
      excerpt: wikiSearchExcerpt(page.content, match),
      page,
    });
  }
  return {
    snapshotId: identity.snapshotId,
    sourceRevision: identity.sourceRevision,
    results: results.slice(0, MAX_WIKI_SEARCH_RESULTS),
    truncated: raw.value.truncated,
  };
}

export function useWikiSearch(input: {
  scope?: OwnerOperationScope;
  coordinate?: string;
  snapshot?: WikiSnapshotRead | null;
  query: string;
}) {
  const normalized = React.useMemo(() => {
    try {
      return normalizeWikiSearchQuery(input.query);
    } catch {
      return "";
    }
  }, [input.query]);
  const debounced = useDebouncedValue(normalized, WIKI_SEARCH_DEBOUNCE_MS);
  const identity = verifiedWikiSnapshotIdentity(
    input.snapshot,
    input.coordinate ?? "",
  );
  const scopeKey = input.scope
    ? wikiSnapshotScopeKey(input.scope)
    : ["", "", -1, -1];
  return useQuery({
    queryKey: [
      "crew-wiki-search",
      ...scopeKey,
      input.coordinate ?? "",
      identity?.snapshotId ?? "",
      identity?.sourceRevision ?? "",
      debounced,
    ],
    enabled: Boolean(
      input.scope &&
        input.coordinate &&
        identity &&
        normalized &&
        debounced === normalized,
    ),
    queryFn: ({ signal }) =>
      searchWikiAtScope({
        scope: input.scope as OwnerOperationScope,
        coordinate: input.coordinate as string,
        snapshot: input.snapshot as WikiSnapshotRead,
        query: debounced,
        signal,
      }),
    retry: false,
    staleTime: 5_000,
    gcTime: 0,
  });
}
