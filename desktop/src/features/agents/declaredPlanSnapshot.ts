import { parseAgentPlanTodos } from "@/features/agents/ui/agentPlanProgress";
import type { ObserverEvent } from "@/features/agents/ui/agentSessionTypes";
import {
  extractToolArgs,
  extractToolIdentity,
} from "@/features/agents/ui/agentSessionTranscriptHelpers";
import { asRecord, asString } from "@/features/agents/ui/agentSessionUtils";
import { normalizeToolNameText } from "@/features/agents/ui/agentSessionToolCatalog";

export type DeclaredPlanStatus = "pending" | "in_progress" | "completed";

export type DeclaredPlanSource = "acp-plan" | "todo-tool";

export type DeclaredPlanEntry = {
  content: string;
  status: DeclaredPlanStatus;
};

export type DeclaredPlanSnapshot = {
  entries: DeclaredPlanEntry[];
  updatedAt: string;
  source: DeclaredPlanSource;
  sessionId: string | null;
  seq: number;
};

const TODO_TOOL_NAMES = new Set([
  "todo",
  "todowrite",
  "todo_write",
  "updateplan",
  "update_plan",
  "plantodo",
]);

/**
 * Structured ACP `sessionUpdate: plan` snapshot, or null when `entries` is
 * absent. Empty `entries` is a real snapshot (clears the card).
 */
export function parseAcpPlanSnapshot(
  update: Record<string, unknown>,
  meta: { updatedAt: string; sessionId: string | null; seq: number },
): DeclaredPlanSnapshot | null {
  if (!Array.isArray(update.entries)) return null;
  const entries: DeclaredPlanEntry[] = [];
  for (const raw of update.entries) {
    const record = asRecord(raw);
    const content = asString(record.content)?.trim();
    if (!content) continue;
    entries.push({
      content,
      status: parseAcpStatus(record.status),
    });
  }
  return {
    entries,
    updatedAt: meta.updatedAt,
    source: "acp-plan",
    sessionId: meta.sessionId,
    seq: meta.seq,
  };
}

/**
 * Fallback structured todo / plan:todo tool snapshot. Maps `{text, done}`
 * without inventing `in_progress`. An explicit `status` of `in_progress` on
 * the tool row is preserved because the adapter declared it.
 */
export function parseTodoToolSnapshot(
  update: Record<string, unknown>,
  meta: { updatedAt: string; sessionId: string | null; seq: number },
): DeclaredPlanSnapshot | null {
  if (!isStructuredTodoTool(update)) return null;
  const args = extractToolArgs(update);
  const todos = parseAgentPlanTodos(args);
  if (!todos) return null;
  const rawTodos = Array.isArray(args.todos) ? args.todos : [];
  const entries = todos.map((todo, index) => {
    const record = asRecord(rawTodos[index]);
    return {
      content: todo.text,
      status: declaredTodoStatus(record, todo.done),
    };
  });
  return {
    entries,
    updatedAt: meta.updatedAt,
    source: "todo-tool",
    sessionId: meta.sessionId,
    seq: meta.seq,
  };
}

/** Latest full-replacement snapshot from one observer event, or null. */
export function snapshotFromObserverEvent(
  event: ObserverEvent,
): DeclaredPlanSnapshot | "clear" | null {
  const update = sessionUpdateFromEvent(event);
  if (!update) return null;
  const meta = {
    updatedAt: event.timestamp,
    sessionId: event.sessionId,
    seq: event.seq,
  };
  const updateType = asString(update.sessionUpdate);
  if (updateType === "plan") {
    const snapshot = parseAcpPlanSnapshot(update, meta);
    if (!snapshot) return null;
    return snapshot.entries.length === 0 ? "clear" : snapshot;
  }
  if (updateType === "tool_call" || updateType === "tool_call_update") {
    return parseTodoToolSnapshot(update, meta);
  }
  return null;
}

export function sessionUpdateFromEvent(
  event: ObserverEvent,
): Record<string, unknown> | null {
  if (event.kind !== "acp_read") return null;
  const payload = asRecord(event.payload);
  if (asString(payload.method) !== "session/update") return null;
  const params = asRecord(payload.params);
  const update = asRecord(params.update);
  return Object.keys(update).length > 0 ? update : null;
}

function parseAcpStatus(value: unknown): DeclaredPlanStatus {
  if (value === "in_progress") return "in_progress";
  if (value === "completed") return "completed";
  return "pending";
}

function declaredTodoStatus(
  record: Record<string, unknown>,
  done: boolean,
): DeclaredPlanStatus {
  const status = asString(record.status)?.toLowerCase();
  if (status === "in_progress") return "in_progress";
  if (status === "completed" || status === "done" || done) return "completed";
  return "pending";
}

function isStructuredTodoTool(update: Record<string, unknown>): boolean {
  const identity = extractToolIdentity(update);
  const names = [identity.toolName, identity.title, identity.buzzToolName];
  return names.some((name) => {
    if (!name) return false;
    const normalized = normalizeToolNameText(name);
    if (TODO_TOOL_NAMES.has(normalized)) return true;
    const compact = normalized.replace(/[^a-z0-9]/g, "");
    return TODO_TOOL_NAMES.has(compact);
  });
}
