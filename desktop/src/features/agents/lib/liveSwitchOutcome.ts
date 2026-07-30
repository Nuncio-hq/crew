import type { ControlResultFrame } from "@/shared/api/types";

/**
 * Resolve the outcome of a live `switch_model` across exact observer turns.
 *
 * A live switch fires a `switch_model` frame per active turn and learns each
 * turn's result asynchronously over the observer relay. The fail-fast rule:
 * any single `unsupported_model` result rejects the whole pick immediately;
 * a terminal/no-active result reports that the switch did not apply, and every
 * successful acknowledgement must arrive exactly once before resolving.
 * Timeout is explicitly unconfirmed rather than a false success.
 *
 * The counting lives here, isolated from React and the relay so it can be unit
 * tested with synthetic frames and a fake clock. The caller injects the
 * relay subscription, the per-turn sends, and the timeout scheduler.
 */
export async function awaitLiveSwitchOutcome({
  targetTurnIds,
  modelId,
  subscribe,
  sendSwitches,
  scheduleTimeout,
}: {
  /** Exact observer turns targeted by the switch. */
  targetTurnIds: readonly string[];
  /** Model being switched to; frames for any other model are ignored. */
  modelId: string;
  /** Register a control-result listener; returns an unsubscribe function. */
  subscribe: (listener: (frame: ControlResultFrame) => void) => () => void;
  /** Fire the per-turn `switch_model` sends. Resolves when all are sent. */
  sendSwitches: () => Promise<void>;
  /** Schedule the no-reply fallback; returns a cancel function. */
  scheduleTimeout: (onTimeout: () => void) => () => void;
}): Promise<"ok" | "unsupported" | "not_applied" | "unconfirmed"> {
  const settled = new Promise<
    "ok" | "unsupported" | "not_applied" | "unconfirmed"
  >((resolve) => {
    let unsubscribe = () => {};
    let cancelTimeout = () => {};
    const remaining = new Set(targetTurnIds);
    const finish = (
      outcome: "ok" | "unsupported" | "not_applied" | "unconfirmed",
    ) => {
      cancelTimeout();
      unsubscribe();
      resolve(outcome);
    };
    cancelTimeout = scheduleTimeout(() => finish("unconfirmed"));
    unsubscribe = subscribe((frame) => {
      if (frame.type !== "switch_model" || frame.modelId !== modelId) {
        return;
      }
      if (!frame.turnId || !remaining.has(frame.turnId)) {
        return;
      }
      if (frame.status === "unsupported_model") {
        // Any single failure rejects the whole pick immediately.
        finish("unsupported");
        return;
      }
      if (frame.status === "turn_ending" || frame.status === "no_active_turn") {
        finish("not_applied");
        return;
      }
      // sent / switched — count each exact turn at most once.
      remaining.delete(frame.turnId);
      if (remaining.size === 0) {
        finish("ok");
      }
    });
  });

  await sendSwitches();

  return settled;
}
