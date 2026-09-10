import {
  estimateWikiSnapshotBytes,
  MAX_AUTOMATIC_WIKI_REPO_READS,
  MAX_RETAINED_WIKI_GRAPH_BYTES,
  type WikiRepositoryReadOutcome,
  type WikiRepositoryReadStatus,
  type WikiRepositorySnapshotCache,
  type WikiSnapshotRead,
} from "@/shared/api/wikiSnapshot";
import {
  sameOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";

export type WikiRepositoryReadPlan = {
  prioritized: string[];
  automatic: string[];
  skipped: string[];
};

export type WikiRepositoryReadResult =
  | { coordinate: string; snapshot: WikiSnapshotRead }
  | {
      coordinate: string;
      error: string;
      outcome?: "error" | "incomplete" | "over-budget" | "preserved";
    };

/**
 * Select explicit/visible repositories first, then consume the fixed
 * automatic refresh budget. The result includes every skipped coordinate so
 * the UI can offer a recovery action instead of silently dropping a repo.
 */
export function planWikiRepositoryReads(
  coordinates: readonly string[],
  priorities: readonly string[] = [],
  automaticLimit = MAX_AUTOMATIC_WIKI_REPO_READS,
): WikiRepositoryReadPlan {
  const unique = [...new Set(coordinates)];
  const prioritySet = new Set(priorities);
  const prioritized = [...new Set(priorities)].filter(
    (coordinate) => prioritySet.has(coordinate) && unique.includes(coordinate),
  );
  const automaticCandidates = unique.filter(
    (coordinate) => !prioritySet.has(coordinate),
  );
  const automatic = automaticCandidates.slice(0, automaticLimit);
  const skipped = automaticCandidates.slice(automatic.length);
  return { prioritized, automatic, skipped };
}

function renderable(
  snapshot: WikiSnapshotRead | null | undefined,
): snapshot is WikiSnapshotRead {
  return snapshot?.state === "complete" || snapshot?.state === "legacy";
}

function messageFor(error: string): string {
  return error.trim() || "Wiki read failed.";
}

function status(
  coordinate: string,
  outcome: WikiRepositoryReadOutcome,
  snapshot: WikiSnapshotRead | null,
  message: string | null,
): WikiRepositoryReadStatus {
  const stale =
    outcome !== "complete" && outcome !== "legacy" && outcome !== "missing";
  const unavailable =
    outcome !== "complete" && outcome !== "legacy" && outcome !== "missing";
  return {
    coordinate,
    state: renderable(snapshot)
      ? stale
        ? "stale"
        : "ready"
      : unavailable
        ? "unavailable"
        : "missing",
    outcome,
    stale,
    unavailable,
    message,
  };
}

function resultByCoordinate(
  results: readonly WikiRepositoryReadResult[],
): Map<string, WikiRepositoryReadResult> {
  return new Map(results.map((result) => [result.coordinate, result]));
}

/**
 * Merge a bounded refresh with same-scope verified cache entries. The byte
 * budget is applied to the complete logical graph before entries are retained;
 * an over-budget result may use a fitting previous graph and is marked stale.
 */
export function projectWikiRepositoryReads(
  scope: OwnerOperationScope,
  coordinates: readonly string[],
  results: readonly WikiRepositoryReadResult[],
  skipped: readonly string[] = [],
  previous = new Map<string, WikiRepositorySnapshotCache>(),
  maxBytes = MAX_RETAINED_WIKI_GRAPH_BYTES,
  priorityOrder: readonly string[] = [],
): Map<string, WikiRepositorySnapshotCache> {
  const byCoordinate = resultByCoordinate(results);
  const skippedSet = new Set(skipped);
  const projected = new Map<string, WikiRepositorySnapshotCache>();
  let retainedBytes = 0;

  const uniqueCoordinates = [...new Set(coordinates)];
  const knownCoordinates = new Set(uniqueCoordinates);
  const projectionOrder = [
    ...new Set([
      ...priorityOrder.filter((coordinate) => knownCoordinates.has(coordinate)),
      ...uniqueCoordinates,
    ]),
  ];

  for (const coordinate of projectionOrder) {
    const previousValue = previous.get(coordinate);
    const previousSnapshot =
      previousValue &&
      sameOwnerOperationScope(previousValue.scope, scope) &&
      renderable(previousValue.snapshot)
        ? previousValue.snapshot
        : null;
    const previousBytes = estimateWikiSnapshotBytes(previousSnapshot);
    const result = byCoordinate.get(coordinate);
    const outcome: WikiRepositoryReadOutcome = skippedSet.has(coordinate)
      ? "skipped"
      : result && "snapshot" in result
        ? result.snapshot.state
        : (result?.outcome ?? "error");
    const resultMessage =
      result && "error" in result ? messageFor(result.error) : null;

    // A coordinator can run a priority-only correction pass while preserving
    // entries that were already accepted by the same scope. Keep the prior
    // entry verbatim after checking both its scope fence and aggregate byte
    // budget; a foreign or over-budget entry must go through the normal
    // unavailable path below.
    //
    // Preservation deliberately does NOT require a renderable graph. A
    // missing, incomplete or errored entry carries real prior status, message
    // and repository metadata for this coordinate, and a correction pass that
    // never re-read it must not overwrite that with a generic "Wiki read
    // failed." unavailable row. Null-snapshot entries contribute
    // serializedBytes = 0, so the aggregate budget is unaffected by keeping
    // them.
    if (
      result &&
      "error" in result &&
      result.outcome === "preserved" &&
      previousValue &&
      sameOwnerOperationScope(previousValue.scope, scope) &&
      retainedBytes + previousValue.serializedBytes <= maxBytes
    ) {
      projected.set(coordinate, previousValue);
      retainedBytes += previousValue.serializedBytes;
      continue;
    }

    const currentSnapshot =
      result && "snapshot" in result && renderable(result.snapshot)
        ? result.snapshot
        : null;
    const currentBytes = estimateWikiSnapshotBytes(currentSnapshot);

    let snapshot: WikiSnapshotRead | null = currentSnapshot;
    let bytes = currentBytes;
    let finalOutcome: WikiRepositoryReadOutcome = outcome;
    let message = resultMessage;
    let repoState =
      result && "snapshot" in result ? result.snapshot.repoState : null;
    let repoStateFresh = Boolean(result && "snapshot" in result);
    const canRetainPrevious = outcome !== "missing";
    if (!snapshot || retainedBytes + bytes > maxBytes) {
      if (
        canRetainPrevious &&
        previousSnapshot &&
        retainedBytes + previousBytes <= maxBytes
      ) {
        snapshot = previousSnapshot;
        bytes = previousBytes;
        finalOutcome =
          outcome === "complete" || outcome === "legacy"
            ? "over-budget"
            : outcome;
        message =
          message ??
          "Showing the last verified Wiki while this refresh is unavailable.";
        // A fresh state ref describes the current relay snapshot even when
        // the body had to fall back to an older verified graph. Keep it
        // independent from the retained graph's provenance. Only an absent
        // current result may carry forward the previous state, and that state
        // remains explicitly stale.
        if (!result || !("snapshot" in result)) {
          repoState = previousValue?.repoState ?? previousSnapshot.repoState;
          repoStateFresh = false;
        }
      } else {
        snapshot = null;
        bytes = 0;
        if (outcome === "complete" || outcome === "legacy") {
          finalOutcome = "over-budget";
          message = "Wiki snapshot exceeds the retained graph budget.";
        } else if (skippedSet.has(coordinate)) {
          message =
            "Automatic Wiki refresh limit reached. Select or retry this repository to load it.";
        }
      }
    }

    if (snapshot) retainedBytes += bytes;
    projected.set(coordinate, {
      scope,
      coordinate,
      snapshot,
      repoState,
      repoStateFresh,
      serializedBytes: bytes,
      status: status(coordinate, finalOutcome, snapshot, message),
    });
  }
  return projected;
}

export function isRenderableWikiSnapshot(
  snapshot: WikiSnapshotRead | null | undefined,
): snapshot is WikiSnapshotRead {
  return renderable(snapshot);
}
