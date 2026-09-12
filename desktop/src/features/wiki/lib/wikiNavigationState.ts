/**
 * Scoped, best-effort Wiki reading state. The key includes the captured
 * viewer/community scope and the containing Project/repository so a saved
 * page can never be reused by another owner, community, or repository.
 */

export type WikiNavigationIdentity = {
  community: string;
  viewer: string;
  projectId: string;
  repositoryCoordinate: string;
  surface: "library" | "project";
};

export type WikiNavigationState = {
  pageId: string | null;
  pageSlug: string | null;
  scrollTop: number;
};

type StoredWikiNavigationState = WikiNavigationState & {
  updatedAt: number;
};

const STORAGE_KEY = "buzz.wiki.navigation.v1";
const MAX_ENTRIES = 64;
const MAX_SCROLL_TOP = 100_000_000;
const MAX_PAGE_VALUE_LENGTH = 512;

function normalizedIdentity(identity: WikiNavigationIdentity): string | null {
  const values = [
    identity.community.trim(),
    identity.viewer.trim().toLowerCase(),
    identity.projectId.trim(),
    identity.repositoryCoordinate.trim(),
    identity.surface,
  ];
  if (values.some((value) => value.length === 0)) return null;
  return values.map((value) => encodeURIComponent(value)).join("|");
}

function validPageValue(value: unknown): string | null {
  return typeof value === "string" && value.length <= MAX_PAGE_VALUE_LENGTH
    ? value || null
    : null;
}

function validScrollTop(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.min(MAX_SCROLL_TOP, Math.max(0, value));
}

function readStoredEntries(): Record<string, StoredWikiNavigationState> {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return {};
    }
    const entries: Record<string, StoredWikiNavigationState> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (!value || typeof value !== "object" || Array.isArray(value)) {
        continue;
      }
      const candidate = value as Partial<StoredWikiNavigationState>;
      if (
        typeof candidate.updatedAt !== "number" ||
        !Number.isFinite(candidate.updatedAt)
      ) {
        continue;
      }
      entries[key] = {
        pageId: validPageValue(candidate.pageId),
        pageSlug: validPageValue(candidate.pageSlug),
        scrollTop: validScrollTop(candidate.scrollTop),
        updatedAt: candidate.updatedAt,
      };
    }
    return entries;
  } catch {
    return {};
  }
}

/** Read the last page and scroll position for one exact Wiki scope. */
export function readWikiNavigationState(
  identity: WikiNavigationIdentity,
): WikiNavigationState | null {
  const key = normalizedIdentity(identity);
  if (!key) return null;
  const entry = readStoredEntries()[key];
  if (!entry) return null;
  return {
    pageId: entry.pageId,
    pageSlug: entry.pageSlug,
    scrollTop: entry.scrollTop,
  };
}

/**
 * Persist one bounded Wiki navigation snapshot. Storage failures are ignored
 * so the in-memory view remains usable when preferences are unavailable.
 */
export function writeWikiNavigationState(
  identity: WikiNavigationIdentity,
  state: WikiNavigationState,
): void {
  const key = normalizedIdentity(identity);
  if (!key) return;
  const entries = readStoredEntries();
  entries[key] = {
    pageId: validPageValue(state.pageId),
    pageSlug: validPageValue(state.pageSlug),
    scrollTop: validScrollTop(state.scrollTop),
    updatedAt: Date.now(),
  };
  const bounded = Object.fromEntries(
    Object.entries(entries)
      .sort(([, left], [, right]) => right.updatedAt - left.updatedAt)
      .slice(0, MAX_ENTRIES),
  );
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(bounded));
  } catch {
    // Persistence is best-effort; the in-memory view still works.
  }
}

/** Exposed for focused tests and migration-safe callers. */
export const WIKI_NAVIGATION_STORAGE_KEY = STORAGE_KEY;
