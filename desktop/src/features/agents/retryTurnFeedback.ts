/**
 * User-visible copy for `retry_turn` control_result statuses.
 * Each status must render differently — silent no-ops are the failure this
 * path exists to remove.
 */

export type RetryTurnStatus =
  | "dispatched"
  | "dispatched_partial"
  | "events_gone"
  | "already_running"
  | "agent_removed"
  | "unconfirmed";

export type RetryTurnFeedback = {
  status: RetryTurnStatus;
  tone: "success" | "info" | "warning" | "error";
  message: string;
};

export function normalizeRetryTurnStatus(status: string): RetryTurnStatus {
  if (
    status === "dispatched" ||
    status === "dispatched_partial" ||
    status === "events_gone" ||
    status === "already_running" ||
    status === "agent_removed"
  ) {
    return status;
  }
  return "unconfirmed";
}

export function describeRetryTurnResult(
  status: string,
  agentName: string,
): RetryTurnFeedback {
  const normalized = normalizeRetryTurnStatus(status);
  switch (normalized) {
    case "dispatched":
      return {
        status: normalized,
        tone: "success",
        message: `Retrying with ${agentName}…`,
      };
    case "dispatched_partial":
      return {
        status: normalized,
        tone: "info",
        message: `Retrying the remaining requests with ${agentName}. Some were withdrawn.`,
      };
    case "events_gone":
      return {
        status: normalized,
        tone: "warning",
        message:
          "Those messages are no longer on the relay — nothing to retry.",
      };
    case "already_running":
      return {
        status: normalized,
        tone: "warning",
        message: `${agentName} is already working on this thread.`,
      };
    case "agent_removed":
      return {
        status: normalized,
        tone: "warning",
        message: `Those requests no longer mention ${agentName}.`,
      };
    case "unconfirmed":
      return {
        status: normalized,
        tone: "warning",
        message: `Retry sent to ${agentName}, but it did not confirm.`,
      };
  }
}
