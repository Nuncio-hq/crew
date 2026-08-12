import type {
  ConnectionState,
  ObserverEvent,
} from "@/features/agents/ui/agentSessionTypes";

/** A live lease is considered lost after three expected 10-second heartbeats. */
export const AGENT_LOST_CONTACT_MS = 30_000;
/** Alive-but-no-progress warning threshold. */
export const AGENT_POSSIBLY_STALLED_MS = 90_000;
/** Explicit typed waits get a bounded calm window before the normal warning. */
export const AGENT_KNOWN_WAIT_GRACE_MS = 5 * 60_000;

export type AgentProgressKind = "progress" | "known-wait";

export type AgentSubstantiveProgress = {
  fingerprint: string;
  kind: AgentProgressKind;
  label: string;
};

export type AgentAttentionTurn = {
  agentPubkey: string;
  anchorAt: number;
  lastSeenAt: number;
  lastSubstantiveProgressAt: number;
  progressKind: AgentProgressKind;
  progressLabel: string;
};

export type AgentAttentionState =
  | "idle"
  | "working"
  | "known-wait"
  | "possibly-stalled"
  | "telemetry-unavailable"
  | "lost-contact"
  | "needs-you"
  | "failed"
  | "ready-to-review"
  | "done";

export type AgentAttentionProjection = {
  state: AgentAttentionState;
  agentPubkey: string | null;
  lastSeenAt: number | null;
  lastSubstantiveProgressAt: number | null;
  lastVerifiedLabel: string | null;
};

type AgentAttentionInput = {
  connectionState: ConnectionState;
  needsYou: boolean;
  now: number;
  outcome: "completed" | "error" | "lost-contact" | null;
  receipt: { createdAt: number; reviewed: boolean } | null;
  /** Runtime lifecycle is listening/asleep; stale observer liveness is calm. */
  sleeping?: boolean;
  snoozedUntil?: number | null;
  turns: readonly AgentAttentionTurn[];
};

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function asString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function asFiniteNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function stableJson(value: unknown): string {
  if (Array.isArray(value)) {
    return `[${value.map(stableJson).join(",")}]`;
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stableJson(record[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value) ?? "null";
}

function toolName(update: Record<string, unknown>): string {
  const meta = asRecord(update._meta);
  return (
    asString(update.title) ??
    asString(update.toolName) ??
    asString(update.name) ??
    asString(meta.buzzToolName) ??
    "tool"
  );
}

/**
 * Return a progress marker only for changes that move the work forward or into
 * a deliberate phase. Heartbeats, token/thought chunks, and usage counters are
 * deliberately excluded.
 */
export function progressFromObserverEvent(
  event: ObserverEvent,
): AgentSubstantiveProgress | null {
  if (event.kind === "turn_started") {
    return {
      fingerprint: "turn_started",
      kind: "progress",
      label: "Turn started",
    };
  }

  if (event.kind === "turn_retrying") {
    const payload = asRecord(event.payload);
    const attempt = asFiniteNumber(payload.attempt) ?? 1;
    const maxAttempts = asFiniteNumber(payload.maxAttempts) ?? 1;
    return {
      fingerprint: `retry:${attempt}:${maxAttempts}`,
      kind: "progress",
      label: `Retrying ${attempt}/${maxAttempts}`,
    };
  }

  if (event.kind !== "acp_read") return null;
  const payload = asRecord(event.payload);
  if (asString(payload.method) !== "session/update") return null;
  const params = asRecord(payload.params);
  const update = asRecord(params.update);
  const updateType = asString(update.sessionUpdate);

  if (updateType === "plan") {
    return {
      fingerprint: `plan:${stableJson(update)}`,
      kind: "progress",
      label: "Plan updated",
    };
  }

  if (updateType !== "tool_call" && updateType !== "tool_call_update") {
    return null;
  }

  const id = asString(update.toolCallId) ?? "unknown";
  const status =
    asString(update.status) ??
    (updateType === "tool_call" ? "executing" : "completed");
  const name = toolName(update);
  const running = status === "pending" || status === "executing";
  const failed = status === "failed" || status === "error";
  return {
    fingerprint: `tool:${id}:${status}`,
    kind: "progress",
    label: running
      ? `Running ${name}`
      : failed
        ? `Failed ${name}`
        : `Completed ${name}`,
  };
}

function baseProjection(
  state: AgentAttentionState,
  turn?: AgentAttentionTurn | null,
): AgentAttentionProjection {
  return {
    state,
    agentPubkey: turn?.agentPubkey ?? null,
    lastSeenAt: turn?.lastSeenAt ?? null,
    lastSubstantiveProgressAt: turn?.lastSubstantiveProgressAt ?? null,
    lastVerifiedLabel: turn?.progressLabel ?? null,
  };
}

function oldestBy(
  turns: readonly AgentAttentionTurn[],
  field: "lastSeenAt" | "lastSubstantiveProgressAt",
): AgentAttentionTurn | null {
  let selected: AgentAttentionTurn | null = null;
  for (const turn of turns) {
    if (!selected || turn[field] < selected[field]) selected = turn;
  }
  return selected;
}

/** Single deterministic priority projection shared by thread and Mission Inbox. */
export function deriveAgentAttention(
  input: AgentAttentionInput,
): AgentAttentionProjection {
  if (input.sleeping) return baseProjection("idle");
  if (input.needsYou) return baseProjection("needs-you", input.turns[0]);
  if (input.outcome === "error") return baseProjection("failed");
  if (input.outcome === "lost-contact") {
    return baseProjection(
      input.connectionState === "open"
        ? "lost-contact"
        : "telemetry-unavailable",
    );
  }

  if (input.turns.length > 0) {
    if (input.connectionState !== "open") {
      return baseProjection("telemetry-unavailable", input.turns[0]);
    }

    const oldestSeen = oldestBy(input.turns, "lastSeenAt");
    if (
      oldestSeen &&
      input.now - oldestSeen.lastSeenAt >= AGENT_LOST_CONTACT_MS
    ) {
      return baseProjection("lost-contact", oldestSeen);
    }

    const oldestProgress = oldestBy(input.turns, "lastSubstantiveProgressAt");
    if (oldestProgress) {
      const progressAge = input.now - oldestProgress.lastSubstantiveProgressAt;
      const threshold =
        oldestProgress.progressKind === "known-wait"
          ? AGENT_KNOWN_WAIT_GRACE_MS
          : AGENT_POSSIBLY_STALLED_MS;
      if (progressAge >= threshold) {
        if ((input.snoozedUntil ?? 0) > input.now) {
          return baseProjection("working", oldestProgress);
        }
        return baseProjection("possibly-stalled", oldestProgress);
      }
    }

    if (input.receipt) {
      return baseProjection(
        input.receipt.reviewed ? "done" : "ready-to-review",
      );
    }

    const knownWait = input.turns.find(
      (turn) => turn.progressKind === "known-wait",
    );
    if (knownWait) return baseProjection("known-wait", knownWait);
    return baseProjection("working", input.turns[0]);
  }

  if (input.receipt) {
    return baseProjection(input.receipt.reviewed ? "done" : "ready-to-review");
  }
  return baseProjection("idle");
}
