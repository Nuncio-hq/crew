/**
 * User-visible copy for `cancel_turn` control_result statuses.
 * Each status must render differently — including `no_active_turn`, which used
 * to be a silent harness warn.
 */

export type CancelTurnStatus =
  | "sent"
  | "cancelled_queued"
  | "no_active_turn"
  | "unconfirmed";

export type CancelTurnFeedback = {
  status: CancelTurnStatus;
  tone: "success" | "info" | "warning" | "error";
  message: string;
};

export function normalizeCancelTurnStatus(status: string): CancelTurnStatus {
  if (
    status === "sent" ||
    status === "cancelled_queued" ||
    status === "no_active_turn"
  ) {
    return status;
  }
  return "unconfirmed";
}

export function describeCancelTurnResult(
  status: string,
  agentName: string,
): CancelTurnFeedback {
  const normalized = normalizeCancelTurnStatus(status);
  switch (normalized) {
    case "sent":
      return {
        status: normalized,
        tone: "info",
        message: `Stopping ${agentName}…`,
      };
    case "cancelled_queued":
      return {
        status: normalized,
        tone: "success",
        message: `Withdrew the request before ${agentName} saw it.`,
      };
    case "no_active_turn":
      return {
        status: normalized,
        tone: "warning",
        message: "Nothing to stop — the agent is already idle.",
      };
    case "unconfirmed":
      return {
        status: normalized,
        tone: "warning",
        message: `Stop signal sent to ${agentName}, but it did not confirm.`,
      };
  }
}

/** Prefer an actionable outcome when several cancel frames return. */
export function pickStrongestCancelTurnStatus(
  statuses: readonly string[],
): CancelTurnStatus {
  let best: CancelTurnStatus = "unconfirmed";
  let bestRank = -1;
  for (const status of statuses) {
    const normalized = normalizeCancelTurnStatus(status);
    const rank =
      normalized === "sent"
        ? 3
        : normalized === "cancelled_queued"
          ? 2
          : normalized === "no_active_turn"
            ? 1
            : 0;
    if (rank > bestRank) {
      best = normalized;
      bestRank = rank;
    }
  }
  return best;
}
