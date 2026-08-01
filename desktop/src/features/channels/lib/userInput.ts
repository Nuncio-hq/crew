import {
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
} from "@/shared/constants/kinds";
import type { RelayEvent } from "@/shared/api/types";

export type UserInputOption = {
  value: string;
  label: string;
  description: string;
};

export type UserInputQuestion = {
  id: string;
  header: string;
  question: string;
  options: UserInputOption[];
  multi_select?: boolean;
  allow_custom_answer?: boolean;
  allow_notes?: boolean;
};

export type UserInputRequest = {
  request_id: string;
  session_id: string;
  turn_id: string;
  channel_id: string;
  tool_call_id?: string | null;
  engine: "claude" | "codex" | string;
  message?: string | null;
  questions: UserInputQuestion[];
};

export type UserInputEvent = {
  event: RelayEvent;
  request: UserInputRequest;
};

export type UserInputAnswerValue =
  | string
  | string[]
  | { selected: string | string[]; choice_notes?: Record<string, string> }
  | null;

export type UserInputAnswers = Record<string, UserInputAnswerValue>;

export function parseUserInputRequest(
  event: RelayEvent,
): UserInputRequest | null {
  if (event.kind !== KIND_AGENT_USER_INPUT_REQUESTED) return null;
  try {
    const value = JSON.parse(event.content) as UserInputRequest;
    if (
      !value ||
      typeof value !== "object" ||
      typeof value.request_id !== "string" ||
      !Array.isArray(value.questions)
    ) {
      return null;
    }
    return value;
  } catch {
    return null;
  }
}

export function getAnswerRequestId(event: RelayEvent): string | null {
  if (event.kind !== KIND_AGENT_USER_INPUT_ANSWER) return null;
  const tag = event.tags.find(([name]) => name === "e");
  return tag?.[1] ?? null;
}

export function dedupeUserInputEvents(
  events: RelayEvent[],
  limit = 200,
): RelayEvent[] {
  const byId = new Map<string, RelayEvent>();
  for (const event of events) {
    if (!byId.has(event.id)) byId.set(event.id, event);
  }
  return [...byId.values()]
    .sort((left, right) => right.created_at - left.created_at)
    .slice(0, limit);
}

export function derivePendingUserInputs(
  events: RelayEvent[],
  currentPubkey: string,
  optimisticallyResolvedIds: ReadonlySet<string> = new Set(),
): UserInputEvent[] {
  const deduped = dedupeUserInputEvents(events);
  const answered = new Set(
    deduped
      .filter(
        (event) =>
          event.kind === KIND_AGENT_USER_INPUT_ANSWER &&
          event.pubkey === currentPubkey,
      )
      .map(getAnswerRequestId)
      .filter((id): id is string => id !== null),
  );
  return deduped
    .filter((event) => event.kind === KIND_AGENT_USER_INPUT_REQUESTED)
    .map((event) => {
      const request = parseUserInputRequest(event);
      return request ? { event, request } : null;
    })
    .filter((item): item is UserInputEvent => item !== null)
    .filter(
      ({ event }) =>
        !answered.has(event.id) && !optimisticallyResolvedIds.has(event.id),
    )
    .sort((left, right) => right.event.created_at - left.event.created_at);
}

export function buildUserInputAnswers(
  values: Record<string, UserInputAnswerValue>,
): string {
  return JSON.stringify(values);
}

export function buildSkippedAnswers(questionIds: string[]): UserInputAnswers {
  return Object.fromEntries(questionIds.map((id) => [id, null]));
}
