/**
 * Coalesce store listener sweeps to at most one flush per animation frame.
 *
 * Mutations stay synchronous. Only the listener callback is deferred, so
 * `useSyncExternalStore` re-reads a complete snapshot at flush time.
 *
 * Hidden documents (and Node tests with no `document`) flush immediately so
 * background processing and replay/restore assertions stay deterministic.
 */

export type CoalesceNotifier = {
  /** Arm a flush; repeated calls before it runs are absorbed. */
  schedule: () => void;
  /** Run the pending flush now (cancels a scheduled rAF). */
  flush: () => void;
};

export type CoalescedHub<Update = void> = {
  subscribe: (listener: (update?: Update) => void) => () => void;
  notify: (update?: Update) => void;
  flush: () => void;
  /** Drop queued payloads without delivering. A later `notify()` still coalesces. */
  clearPending: () => void;
  readonly listenerCount: number;
};

const notifiers: CoalesceNotifier[] = [];

function shouldFlushImmediately(): boolean {
  if (typeof document === "undefined") return true;
  return document.hidden === true;
}

/**
 * Wrap `notify` so the first trigger schedules a microtask + rAF cap and
 * later triggers in the same frame are absorbed.
 */
export function coalesceNotifier(notify: () => void): CoalesceNotifier {
  let scheduled = false;
  let rafId: number | null = null;

  const flush = () => {
    if (rafId != null) {
      if (typeof cancelAnimationFrame === "function") {
        cancelAnimationFrame(rafId);
      }
      rafId = null;
    }
    if (!scheduled) return;
    scheduled = false;
    notify();
  };

  const schedule = () => {
    if (shouldFlushImmediately()) {
      scheduled = true;
      flush();
      return;
    }
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(() => {
      if (!scheduled) return;
      if (typeof requestAnimationFrame === "function") {
        rafId = requestAnimationFrame(() => {
          rafId = null;
          flush();
        });
        return;
      }
      flush();
    });
  };

  const notifier = { schedule, flush };
  notifiers.push(notifier);
  return notifier;
}

/**
 * Drain every coalesced notifier. Tests that count listener invocations
 * after a burst of mutations should call this before asserting.
 */
export function flushPendingNotificationsForTest(): void {
  for (const notifier of notifiers) notifier.flush();
}

/**
 * Merge consecutive (or interleaved) observer-store publications for the
 * same agent into one payload so a coalesced sweep still carries every
 * newly admitted event.
 */
export function mergeUpdatesByAgentPubkey<
  T extends { agentPubkey: string; events: readonly unknown[] },
>(pending: T[], next: T): void {
  const index = pending.findIndex(
    (item) => item.agentPubkey === next.agentPubkey,
  );
  if (index === -1) {
    pending.push(next);
    return;
  }
  const current = pending[index];
  pending[index] = {
    ...current,
    events: current.events.concat(next.events),
  };
}

/**
 * Listener set whose `notify` is coalesced. Generic `notify()` (no payload)
 * is a connection-state-style sweep; payloads are merged then delivered.
 */
export function createCoalescedHub<Update = void>(options?: {
  merge?: (pending: Update[], next: Update) => void;
}): CoalescedHub<Update> {
  const listeners = new Set<(update?: Update) => void>();
  const pending: Update[] = [];
  let pendingGeneric = false;

  const deliver = (update?: Update) => {
    for (const listener of listeners) listener(update);
  };

  const notifier = coalesceNotifier(() => {
    const updates = pending.splice(0, pending.length);
    const generic = pendingGeneric;
    pendingGeneric = false;
    if (updates.length === 0) {
      if (generic) deliver();
      return;
    }
    for (const update of updates) deliver(update);
  });

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    notify(update?: Update) {
      if (update !== undefined) {
        if (options?.merge) options.merge(pending, update);
        else pending.push(update);
      } else {
        pendingGeneric = true;
      }
      notifier.schedule();
    },
    flush: notifier.flush,
    clearPending() {
      pending.length = 0;
      pendingGeneric = false;
    },
    get listenerCount() {
      return listeners.size;
    },
  };
}
