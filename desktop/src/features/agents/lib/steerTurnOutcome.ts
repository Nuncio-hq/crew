import type { ControlResultFrame } from "@/shared/api/types";

/** Strict selected-run steer outcomes reported by the buzz-agent adapter. */
export type SteerTurnOutcome =
  | "appended"
  | "stale_target"
  | "rejected"
  | "busy"
  | "expired"
  | "unconfirmed";

/** Wait for the exact selected-run steer result, with a bounded UI wait. */
export async function awaitSteerTurnOutcome({
  requestId,
  channelId,
  conversationId,
  sessionId,
  turnId,
  subscribe,
  sendSteer,
  scheduleTimeout,
}: {
  requestId: string;
  channelId: string;
  conversationId: string;
  sessionId: string;
  turnId: string;
  subscribe: (listener: (frame: ControlResultFrame) => void) => () => void;
  sendSteer: () => Promise<void>;
  scheduleTimeout: (onTimeout: () => void) => () => void;
}): Promise<SteerTurnOutcome> {
  let settled = false;
  let unsubscribe = () => {};
  let cancelTimeout = () => {};
  let resolveResult: (outcome: SteerTurnOutcome) => void = () => {};
  let rejectResult: (error: unknown) => void = () => {};
  const result = new Promise<SteerTurnOutcome>((resolve, reject) => {
    resolveResult = resolve;
    rejectResult = reject;
  });
  const cleanup = () => {
    unsubscribe();
    cancelTimeout();
  };
  const settle = (outcome: SteerTurnOutcome) => {
    if (settled) return;
    settled = true;
    cleanup();
    resolveResult(outcome);
  };
  const fail = (error: unknown) => {
    if (settled) return;
    settled = true;
    cleanup();
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
      settle(frame.status);
    }
  });
  // The publication result only proves relay acceptance. A bounded wait for
  // the correlated adapter result keeps a hung relay/agent from leaving the
  // control pending forever; an unconfirmed request must never be replayed.
  cancelTimeout = scheduleTimeout(() => settle("unconfirmed"));
  try {
    void Promise.resolve(sendSteer()).catch(fail);
  } catch (error) {
    fail(error);
  }
  return result;
}
