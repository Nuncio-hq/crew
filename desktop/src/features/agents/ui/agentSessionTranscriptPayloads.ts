/**
 * Pure payload readers for the ACP observer transcript.
 *
 * Extracted from `agentSessionTranscript.ts`: none of these touch the
 * transcript draft, so keeping them beside the reducer only made that file
 * larger than its size budget allows.
 */
import { asRecord, asString, titleCase } from "./agentSessionUtils";

export function maybeNostrEventId(id: string | null | undefined) {
  return id && /^[0-9a-fA-F]{64}$/.test(id) ? id : null;
}

export function stringifyPayload(value: unknown) {
  try {
    return JSON.stringify(value, null, 2) ?? String(value);
  } catch {
    return String(value);
  }
}

export function describePermissionRequest(payload: Record<string, unknown>) {
  const params = asRecord(payload.params);
  const title =
    asString(params.title) ??
    asString(params.message) ??
    asString(params.reason) ??
    "Permission requested";
  const toolCallId =
    asString(params.toolCallId) ?? asString(params.tool_call_id);
  const options = Array.isArray(params.options)
    ? params.options
        .map((option) => {
          const record = asRecord(option);
          return (
            asString(record.name) ??
            asString(record.kind) ??
            asString(record.optionId)
          );
        })
        .filter((option): option is string => Boolean(option))
    : [];
  const detail: string[] = [];
  if (title !== "Permission requested") detail.push(title);
  if (toolCallId) detail.push(`Tool call: ${toolCallId}`);
  if (options.length > 0) detail.push(`Options: ${options.join(", ")}`);

  // Build optionId → kind map for outcome labeling on the response.
  const optionNames = new Map<string, string>();
  if (Array.isArray(params.options)) {
    for (const option of params.options) {
      const record = asRecord(option);
      const optionId = asString(record.optionId);
      const kind = asString(record.kind);
      if (optionId && kind) {
        optionNames.set(optionId, kind);
      }
    }
  }

  return {
    title,
    text: detail.join("\n"),
    optionNames,
    descriptor: {
      renderClass: "permission" as const,
      label: "Permission requested",
      preview: title,
      action: { verb: "Requested", object: title },
      tone: "admin" as const,
      operation: "session/request_permission",
      object: title,
      source: "acp" as const,
      groupKey: "permission:request",
    },
  };
}

/**
 * Format a human-readable outcome label from a permission response.
 * kind values from ACP: allow_once, allow_always, reject_once, reject_always.
 * "reject_*" kinds are denials; anything else that is selected is an approval.
 */
export function describePermissionOutcome(
  outcome: string,
  optionId: string | null,
  optionNames: Map<string, string>,
): string {
  if (outcome === "cancelled") {
    return "Cancelled";
  }
  if (outcome === "selected" && optionId) {
    const kind = optionNames.get(optionId) ?? optionId;
    const isDenial = kind.startsWith("reject");
    const verb = isDenial ? "Denied" : "Approved";
    return `${verb} (${kind})`;
  }
  return outcome;
}

/**
 * Stable map key for a JSON-RPC id, which may be a string or a finite number
 * per the spec. Using JSON.stringify avoids collisions between the number 1 and
 * the string "1". Returns null for null, undefined, or non-id values (objects,
 * booleans) so callers can gate on presence without a separate type check.
 */
export function jsonRpcId(value: unknown): string | null {
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "number" && Number.isFinite(value))
    return JSON.stringify(value);
  return null;
}

export function describeFreeformStatus(payload: Record<string, unknown>) {
  const statusType = asString(payload.type) ?? asString(payload.status);
  const title =
    asString(payload.title) ?? (statusType ? titleCase(statusType) : null);
  const text = asString(payload.text) ?? asString(payload.message);
  if (!title || !text) return null;
  return { statusType: statusType ?? title.toLowerCase(), title, text };
}

export function rawPayloadTitle(payload: unknown) {
  const record = asRecord(payload);
  return asString(record.method) ?? asString(record.type) ?? "raw_json_rpc";
}
