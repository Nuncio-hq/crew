import type { ControlResultFrame } from "@/shared/api/types";

/**
 * The strict steer deadline the buzz-agent adapter enforces per request.
 *
 * Mirrors `REQUEST_DEADLINE` in `crates/buzz-agent/src/strict_steer.rs`. The
 * adapter only reports `expired` once this elapses, so any UI wait shorter
 * than or equal to it can never observe that outcome.
 */
export const ADAPTER_STRICT_STEER_DEADLINE_MS = 10_000;

/**
 * Headroom over the adapter deadline for relay fan-out and render time.
 *
 * Without it the two budgets race and the adapter's terminal answer always
 * loses, which downgrades a retryable `expired` to a latched `unconfirmed`.
 */
const STEER_UI_BUDGET_HEADROOM_MS = 5_000;

/** How long the control waits before reporting the steer as unconfirmed. */
export const STEER_UI_BUDGET_MS =
  ADAPTER_STRICT_STEER_DEADLINE_MS + STEER_UI_BUDGET_HEADROOM_MS;

/**
 * Largest steer prompt the native control accepts, in UTF-8 bytes.
 *
 * Mirrors the native caps in `scoped_observer_control.rs` and
 * `crew_thread_cancel.rs`, both of which count bytes. A character count would
 * admit a prompt the adapter then refuses.
 */
export const STEER_PROMPT_MAX_BYTES = 16 * 1024;

/** UTF-8 byte length of a steer prompt, matching the native cap's unit. */
export function steerPromptByteLength(prompt: string): number {
  return new TextEncoder().encode(prompt).length;
}

/** Strict selected-run steer outcomes reported by the buzz-agent adapter. */
export type SteerTurnOutcome =
  | "appended"
  | "stale_target"
  | "rejected"
  | "busy"
  | "expired"
  | "unconfirmed";

/** Correlated strict steer outcome plus the adapter's actionable detail. */
export type SteerTurnOutcomeResult = Readonly<{
  outcome: SteerTurnOutcome;
  error?: string;
}>;

/** A steer wait that outlives its own UI budget until it is disposed. */
export type PendingSteerTurnOutcome = Readonly<{
  result: Promise<SteerTurnOutcomeResult>;
  dispose: () => void;
}>;

/**
 * Wait for the exact selected-run steer result.
 *
 * The awaited promise is bounded so a hung relay or agent cannot leave the
 * control pending forever. The correlated subscription deliberately outlives
 * that budget: the adapter's own deadline can elapse after the UI has already
 * reported `unconfirmed`, and that late terminal outcome is what proves the
 * request is safe to retry. Late outcomes go to `onLateOutcome`; the caller
 * must `dispose()` when the control goes away.
 */
export function awaitSteerTurnOutcome({
  requestId,
  channelId,
  conversationId,
  sessionId,
  turnId,
  subscribe,
  sendSteer,
  scheduleTimeout,
  onLateOutcome,
}: {
  requestId: string;
  channelId: string;
  conversationId: string;
  sessionId: string;
  turnId: string;
  subscribe: (listener: (frame: ControlResultFrame) => void) => () => void;
  sendSteer: () => Promise<void>;
  scheduleTimeout: (onTimeout: () => void) => () => void;
  onLateOutcome?: (result: SteerTurnOutcomeResult) => void;
}): PendingSteerTurnOutcome {
  // `settled` is about the awaited promise; `finished` is about the
  // correlation subscription. The UI budget settles the first without ending
  // the second, which is the whole point of the late-outcome path.
  let settled = false;
  let finished = false;
  let unsubscribe = () => {};
  let cancelTimeout = () => {};
  let resolveResult: (result: SteerTurnOutcomeResult) => void = () => {};
  let rejectResult: (error: unknown) => void = () => {};
  const result = new Promise<SteerTurnOutcomeResult>((resolve, reject) => {
    resolveResult = resolve;
    rejectResult = reject;
  });
  const dispose = () => {
    if (finished) return;
    finished = true;
    unsubscribe();
    cancelTimeout();
    // A request that never lands stays unconfirmed with no replay.
    if (!settled) {
      settled = true;
      resolveResult({ outcome: "unconfirmed" });
    }
  };
  const toResult = (
    outcome: SteerTurnOutcome,
    error?: unknown,
  ): SteerTurnOutcomeResult => {
    const detail = typeof error === "string" ? error.trim() : undefined;
    return detail ? { outcome, error: detail } : { outcome };
  };
  const settle = (outcome: SteerTurnOutcome, error?: unknown) => {
    if (finished) return;
    const terminal = toResult(outcome, error);
    finished = true;
    unsubscribe();
    cancelTimeout();
    if (settled) {
      onLateOutcome?.(terminal);
      return;
    }
    settled = true;
    resolveResult(terminal);
  };
  const fail = (error: unknown) => {
    if (settled || finished) return;
    settled = true;
    finished = true;
    unsubscribe();
    cancelTimeout();
    rejectResult(error);
  };

  unsubscribe = subscribe((frame) => {
    if (
      frame.type !== "steer_turn" ||
      frame.requestId !== requestId ||
      frame.channelId !== channelId ||
      frame.conversationId !== conversationId ||
      frame.sessionId !== sessionId ||
      frame.turnId !== turnId
    ) {
      return;
    }
    if (
      frame.status === "appended" ||
      frame.status === "stale_target" ||
      frame.status === "rejected" ||
      frame.status === "busy" ||
      frame.status === "expired" ||
      frame.status === "unconfirmed"
    ) {
      settle(frame.status, frame.error);
    }
  });
  // The publication result only proves relay acceptance. The bounded wait
  // keeps the control usable; it does not end the correlation.
  cancelTimeout = scheduleTimeout(() => {
    if (settled || finished) return;
    settled = true;
    resolveResult({ outcome: "unconfirmed" });
  });
  try {
    void Promise.resolve(sendSteer()).catch(fail);
  } catch (error) {
    fail(error);
  }
  return { result, dispose };
}
