import type { ManagedAgentRuntimeStatus } from "@/shared/api/types";
import { agentCommunityAvailability } from "./managedAgentRuntimeStatus";

export type ManagedTransportPresentation = {
  label: string;
  detail: string | null;
  needsRetry: boolean;
};

export function managedAgentTransportPresentation(
  runtime: ManagedAgentRuntimeStatus | undefined,
): ManagedTransportPresentation | null {
  const transport = runtime?.transport;
  if (!runtime || !transport || !runtime.localSetup) return null;
  if (runtime.lifecycle === "stopped" && !runtime.transportRetired) return null;
  const labels = {
    unknown: "Connection unknown",
    connecting: "Connecting to relay",
    connected: "Relay connected",
    degraded: "Reconnecting to relay",
    exhausted: "Connection retries exhausted",
    auth_rejected: "Authentication denied",
  };
  const fallback = {
    unknown: "Local connection status is unavailable. Retry by restarting.",
    connecting: `Attempt ${transport.attempts} of 6.`,
    connected: "",
    degraded: `Reconnecting, attempt ${transport.attempts} of 6.`,
    exhausted: "The connection retry limit was reached.",
    auth_rejected: "Review credentials and relay configuration.",
  };
  let detail = transport.lastError ?? fallback[transport.state];
  if (runtime.transportRetired) {
    detail = `Last result from the previous process. ${detail}`;
  } else if (transport.nextRetryAtMs !== null) {
    const next = new Date(transport.nextRetryAtMs);
    if (Number.isFinite(next.getTime())) {
      detail += ` Next retry around ${next.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}.`;
    }
  }
  return {
    label: `${runtime.transportRetired ? "Last process: " : ""}${labels[transport.state]}`,
    detail: detail || null,
    needsRetry:
      transport.state !== "connected" &&
      (runtime.pid !== null ||
        runtime.lifecycle === "failed" ||
        Boolean(runtime.transportRetired)),
  };
}

export function managedAgentProcessLabel(
  runtime: ManagedAgentRuntimeStatus,
): string {
  if (
    runtime.transport &&
    runtime.transport.state !== "connected" &&
    runtime.pid !== null
  ) {
    if (runtime.lifecycle === "ready") return "Running locally";
    if (runtime.lifecycle === "listening") return "Sleeping locally";
  }
  return agentCommunityAvailability(runtime);
}
