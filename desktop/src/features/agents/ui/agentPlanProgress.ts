import type {
  AgentPlanTodo,
  TranscriptItem,
} from "@/features/agents/ui/agentSessionTypes";
import {
  asRecord,
  getToolString,
} from "@/features/agents/ui/agentSessionUtils";

export type AgentPlanProgress = {
  steps: AgentPlanTodo[];
  /** Index of the first open step, or `steps.length` when all are done. */
  currentIndex: number;
  updatedAt: string;
};

/**
 * Parse harness todo args into `{text, done}[]`.
 * Returns null for missing/empty/malformed shapes so callers can degrade to a
 * one-line activity view instead of inventing checklist rows.
 */
export function parseAgentPlanTodos(
  args: Record<string, unknown> | null | undefined,
): AgentPlanTodo[] | null {
  if (!args || typeof args !== "object") return null;
  const todos = args.todos;
  if (!Array.isArray(todos) || todos.length === 0) return null;

  const steps: AgentPlanTodo[] = [];
  for (const todo of todos) {
    if (!todo || typeof todo !== "object") return null;
    const record = asRecord(todo);
    const text = getToolString(record, ["text", "content", "label", "title"]);
    if (!text) return null;
    steps.push({ text, done: isTodoDone(record) });
  }
  return steps.length > 0 ? steps : null;
}

/**
 * Derive the latest plan checklist from an agent transcript.
 * Newest todo/plan snapshot wins. Returns null when no usable checklist exists.
 */
export function getAgentPlanProgress(
  transcript: readonly TranscriptItem[],
  options?: {
    channelId?: string | null;
    turnId?: string | null;
  },
): AgentPlanProgress | null {
  const channelId = options?.channelId?.trim() || null;
  const turnId = options?.turnId?.trim() || null;

  for (let i = transcript.length - 1; i >= 0; i--) {
    const item = transcript[i];
    if (!item) continue;
    if (channelId && item.channelId && item.channelId !== channelId) continue;
    if (turnId && item.turnId && item.turnId !== turnId) continue;

    const steps = stepsFromTranscriptItem(item);
    if (!steps) continue;

    return {
      steps,
      currentIndex: firstOpenIndex(steps),
      updatedAt: item.timestamp,
    };
  }

  return null;
}

function stepsFromTranscriptItem(item: TranscriptItem): AgentPlanTodo[] | null {
  if (item.type === "plan") {
    if (item.isUpdate) return null;
    if (item.todos && item.todos.length > 0) return item.todos;
    return null;
  }

  if (item.type !== "tool") return null;
  if (item.descriptor.todos && item.descriptor.todos.length > 0) {
    return item.descriptor.todos;
  }
  if (
    item.descriptor.renderClass === "plan" ||
    item.descriptor.groupKey === "plan:todo" ||
    item.toolName === "todo"
  ) {
    return parseAgentPlanTodos(item.args);
  }
  return null;
}

function firstOpenIndex(steps: readonly AgentPlanTodo[]): number {
  const index = steps.findIndex((step) => !step.done);
  return index === -1 ? steps.length : index;
}

function isTodoDone(record: Record<string, unknown>): boolean {
  if (typeof record.done === "boolean") return record.done;
  if (typeof record.checked === "boolean") return record.checked;
  const status = getToolString(record, ["status"])?.toLowerCase();
  return status === "completed" || status === "done";
}
