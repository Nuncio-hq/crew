import * as React from "react";

/**
 * Maps a mounted thread composer's `threadRootId` to its own imperative
 * focus function.
 *
 * Steer surfaces outside the composer (e.g. `LiveJobDesk`) need to focus the
 * thread's composer without depending on its DOM shape or a test-only
 * selector — `document.querySelector("[data-testid=...] [contenteditable]")`
 * silently no-ops the moment that markup changes, and cannot distinguish
 * "not mounted yet" from "mounted, but focus failed". Registering the real
 * `richText.focus` here gives callers both: a stable seam, and a signal (no
 * entry) to disable the control instead of silently doing nothing.
 */
const registry = new Map<string, () => void>();
const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) listener();
}

/**
 * Registers the composer mounted for `threadRootId`. Returns an unregister
 * function that must be called on unmount or when `threadRootId` changes —
 * it only clears the entry if it still points at this registration, so a
 * stale unregister from an old composer can never clobber the one that
 * replaced it.
 */
export function registerThreadComposerFocus(
  threadRootId: string,
  focus: () => void,
): () => void {
  registry.set(threadRootId, focus);
  notify();
  return () => {
    if (registry.get(threadRootId) === focus) {
      registry.delete(threadRootId);
      notify();
    }
  };
}

function getSnapshot(threadRootId: string) {
  return registry.get(threadRootId) ?? null;
}

/** The registered composer's focus function, or null when none is mounted. */
export function useThreadComposerFocus(
  threadRootId: string | null,
): (() => void) | null {
  const subscribe = React.useCallback((listener: () => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }, []);
  const getSnapshotFor = React.useCallback(
    () => (threadRootId ? getSnapshot(threadRootId) : null),
    [threadRootId],
  );
  return React.useSyncExternalStore(subscribe, getSnapshotFor, getSnapshotFor);
}

/** Test-only: clears every registration. */
export function resetThreadComposerFocusRegistryForTests() {
  registry.clear();
  notify();
}
