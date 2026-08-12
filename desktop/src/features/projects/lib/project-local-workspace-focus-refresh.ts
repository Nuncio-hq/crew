import {
  createTrailingDebounce,
  type TrailingDebounce,
} from "@/shared/lib/trailingDebounce";

/**
 * Focus-triggered re-read of the D-015 exact local workspace snapshot
 * (issue #139 / D-037). Mirrors the focus + visibility pattern used by
 * `useRelayResumeTriggers`, with a trailing debounce so rapid focus/blur
 * cycles collapse into one point-in-time read.
 *
 * Debounce delay matches channel-list invalidation (`CHANNELS_INVALIDATE_DEBOUNCE_MS`).
 * Min interval matches resume-trigger rate limiting so a sustained focus flap
 * cannot thrash the native reader after a successful re-read.
 */
export const LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS = 500;
export const LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS = 5_000;

/** React Query key segment used by `useProjectLocalRepoSnapshotQuery`. */
export const LOCAL_REPO_SNAPSHOT_QUERY_PART = "local-repo-snapshot";

export function isLocalRepoSnapshotQueryKey(
  queryKey: readonly unknown[],
): boolean {
  return (
    queryKey[0] === "project" && queryKey[2] === LOCAL_REPO_SNAPSHOT_QUERY_PART
  );
}

export function shouldScheduleLocalWorkspaceSnapshotFocusRefresh(inputs: {
  lastRefreshAt: number;
  now: number;
  minIntervalMs?: number;
}): boolean {
  const minInterval =
    inputs.minIntervalMs ?? LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS;
  return inputs.now - inputs.lastRefreshAt >= minInterval;
}

type TimerHost = {
  setTimeout: (handler: () => void, ms: number) => number;
  clearTimeout: (id: number) => void;
};

export type LocalWorkspaceSnapshotFocusRefresh = {
  /** Handle a focus / visibility→visible signal. */
  onAppFocus: () => void;
  /** Drop any pending debounced refresh (e.g. on unmount). */
  cancel: () => void;
};

/**
 * Builds a focus-refresh controller: each focus signal arms a trailing
 * debounce; when the quiet window elapses and the min interval allows it,
 * `refresh` runs once (typically `invalidateQueries` for active local
 * snapshot queries).
 */
export function createLocalWorkspaceSnapshotFocusRefresh(
  refresh: () => void,
  options: {
    debounceMs?: number;
    minIntervalMs?: number;
    now?: () => number;
    host?: TimerHost;
  } = {},
): LocalWorkspaceSnapshotFocusRefresh {
  const debounceMs =
    options.debounceMs ?? LOCAL_WORKSPACE_SNAPSHOT_FOCUS_DEBOUNCE_MS;
  const minIntervalMs =
    options.minIntervalMs ?? LOCAL_WORKSPACE_SNAPSHOT_FOCUS_MIN_INTERVAL_MS;
  const now = options.now ?? Date.now;
  let lastRefreshAt = -Infinity;

  const runIfAllowed = () => {
    const current = now();
    if (
      !shouldScheduleLocalWorkspaceSnapshotFocusRefresh({
        lastRefreshAt,
        now: current,
        minIntervalMs,
      })
    ) {
      return;
    }
    lastRefreshAt = current;
    refresh();
  };

  const debounce: TrailingDebounce = createTrailingDebounce(
    runIfAllowed,
    debounceMs,
    options.host,
  );

  return {
    onAppFocus: () => {
      debounce.trigger();
    },
    cancel: () => {
      debounce.cancel();
    },
  };
}
