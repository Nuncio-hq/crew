/**
 * #367 — the source backlink carried on a dispatched Wiki task kickoff.
 *
 * The kickoff event's tags are deliberately minimal: `["client",
 * "crew-wiki-task", <op-id>]` names the durable operation (the author's own
 * journal resolves it back to the private question/attempt) and
 * `["client", "crew-wiki-task-origin", <coordinate>]` carries the repository
 * coordinate. No question id, attempt id, or private-history locator is ever
 * published — a second viewer can only follow the coordinate to the Wiki
 * surface their own access allows.
 */
export const WIKI_TASK_CLIENT_MARKER = "crew-wiki-task";
export const WIKI_TASK_ORIGIN_MARKER = "crew-wiki-task-origin";

export type WikiTaskOrigin = {
  /** The `thread-handoff` operation id — author-local lookup key. */
  dispatchId: string;
  /** `<owner-hex>:<repo-d>` — the only provenance everyone sees. */
  coordinate: string;
};

export function readWikiTaskOrigin(
  tags: string[][] | undefined | null,
): WikiTaskOrigin | null {
  if (!Array.isArray(tags)) return null;
  let dispatchId: string | null = null;
  let coordinate: string | null = null;
  for (const tag of tags) {
    if (!Array.isArray(tag) || tag[0] !== "client") continue;
    if (tag[1] === WIKI_TASK_CLIENT_MARKER && typeof tag[2] === "string") {
      dispatchId = tag[2];
    } else if (
      tag[1] === WIKI_TASK_ORIGIN_MARKER &&
      typeof tag[2] === "string"
    ) {
      coordinate = tag[2];
    }
  }
  if (!dispatchId || !coordinate || !coordinate.includes(":")) return null;
  return { coordinate, dispatchId };
}

/**
 * Author-side restore: the click stashes the private attempt id BEFORE
 * navigating, and the private-ask composer consumes it once its scoped
 * history arrives. Module-local (not route state) because the route can
 * resolve to any of the wiki surfaces; read-once so a stale hint never
 * re-fires.
 */
let pendingFocus: { attemptId: string; coordinate: string } | null = null;

export function stashWikiAskFocus(attemptId: string, coordinate: string): void {
  pendingFocus = { attemptId, coordinate };
}

/** Read without consuming — the composer peeks until its history is loaded. */
export function peekWikiAskFocus(
  coordinate: string,
): { attemptId: string } | null {
  if (!pendingFocus || pendingFocus.coordinate !== coordinate) return null;
  return { attemptId: pendingFocus.attemptId };
}

/** Consume the pending hint once its coordinate's history has arrived. */
export function takeWikiAskFocus(coordinate: string): void {
  if (pendingFocus?.coordinate === coordinate) {
    pendingFocus = null;
  }
}
