import {
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_RESOLVED,
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
  required?: boolean;
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

export type UserInputResolutionOutcome = "answered" | "declined" | "cancelled";

export type UserInputResolved = {
  request_event_id: string;
  outcome: UserInputResolutionOutcome;
};

export type UserInputAnswerValue =
  | string
  | string[]
  | { selected: string | string[]; choice_notes?: Record<string, string> }
  | null;

export type UserInputAnswers = Record<string, UserInputAnswerValue>;

export async function publishUserInputAnswer(
  publish: (
    channelId: string,
    requestEventId: string,
    answers: UserInputAnswers,
  ) => Promise<unknown>,
  channelId: string,
  requestEventId: string,
  answers: UserInputAnswers,
): Promise<string | null> {
  try {
    await publish(channelId, requestEventId, answers);
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : "Failed to send answer.";
  }
}

export type UserInputDraft = {
  selected: string[];
  custom: string;
  notes: Record<string, string>;
};

export const emptyUserInputDraft = (): UserInputDraft => ({
  selected: [],
  custom: "",
  notes: {},
});

export function canSubmitUserInput(
  questions: UserInputQuestion[],
  drafts: Record<string, UserInputDraft>,
): boolean {
  const hasAnswer = questions.some((question) => {
    const value = drafts[question.id];
    return Boolean(value?.custom.trim() || value?.selected.length);
  });
  return (
    hasAnswer &&
    questions.every((question) => {
      if (!question.required) return true;
      const value = drafts[question.id];
      return Boolean(value?.custom.trim() || value?.selected.length);
    })
  );
}

export function selectUserInputOption(
  question: UserInputQuestion,
  draft: UserInputDraft,
  value: string,
): UserInputDraft {
  const selected = question.multi_select
    ? draft.selected.includes(value)
      ? draft.selected.filter((item) => item !== value)
      : [...draft.selected, value]
    : [value];
  const selectedSet = new Set(selected);
  return {
    selected,
    custom: "",
    notes: Object.fromEntries(
      Object.entries(draft.notes).filter(([key]) => selectedSet.has(key)),
    ),
  };
}

export function setUserInputCustom(
  draft: UserInputDraft,
  custom: string,
): UserInputDraft {
  return custom.trim()
    ? { selected: [], custom, notes: {} }
    : { ...draft, custom };
}

export function buildQuestionAnswer(
  question: UserInputQuestion,
  draft: UserInputDraft,
): UserInputAnswerValue {
  if (draft.custom.trim()) return draft.custom.trim();
  if (question.allow_notes && Object.keys(draft.notes).length > 0) {
    return {
      selected: question.multi_select ? draft.selected : draft.selected[0],
      choice_notes: draft.notes,
    };
  }
  return question.multi_select ? draft.selected : draft.selected[0];
}

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

/** Resolve the canonical triggering thread root carried by a durable request. */
export function deriveUserInputRootEventId(event: RelayEvent): string | null {
  const eventTags = event.tags.filter((tag) => tag[0] === "e");
  if (eventTags.length < 1 || eventTags.length > 2) return null;
  if (
    eventTags.some(
      (tag) =>
        tag.length !== 4 ||
        !/^[0-9a-f]{64}$/.test(tag[1] ?? "") ||
        !["root", "reply"].includes(tag[3] ?? ""),
    )
  ) {
    return null;
  }
  const rootTags = eventTags.filter((tag) => tag[3] === "root");
  const replyTags = eventTags.filter((tag) => tag[3] === "reply");
  if (eventTags.length === 1) {
    return replyTags.length === 1 ? (replyTags[0]?.[1] ?? null) : null;
  }
  if (
    rootTags.length !== 1 ||
    replyTags.length !== 1 ||
    rootTags[0]?.[1] === replyTags[0]?.[1]
  ) {
    return null;
  }
  return rootTags[0]?.[1] ?? null;
}

export function getAnswerRequestId(event: RelayEvent): string | null {
  if (event.kind !== KIND_AGENT_USER_INPUT_ANSWER) return null;
  const tag = event.tags.find(([name]) => name === "e");
  return tag?.[1] ?? null;
}

export function getResolvedRequestId(event: RelayEvent): string | null {
  if (event.kind !== KIND_AGENT_USER_INPUT_RESOLVED) return null;
  const tag = event.tags.find(([name]) => name === "e");
  return tag?.[1] ?? null;
}

export function parseUserInputResolution(
  event: RelayEvent,
): UserInputResolved | null {
  if (event.kind !== KIND_AGENT_USER_INPUT_RESOLVED) return null;
  try {
    const value = JSON.parse(event.content) as UserInputResolved;
    if (
      !value ||
      typeof value !== "object" ||
      typeof value.request_event_id !== "string" ||
      !["answered", "declined", "cancelled"].includes(value.outcome)
    ) {
      return null;
    }
    return value;
  } catch {
    return null;
  }
}

export function dedupeUserInputEvents(
  events: RelayEvent[],
  limit?: number,
): RelayEvent[] {
  const byId = new Map<string, RelayEvent>();
  for (const event of events) {
    if (!byId.has(event.id)) byId.set(event.id, event);
  }
  const sorted = [...byId.values()].sort(
    (left, right) =>
      right.created_at - left.created_at || right.id.localeCompare(left.id),
  );
  return limit === undefined ? sorted : sorted.slice(0, limit);
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
  const resolved = new Map<string, UserInputResolutionOutcome>();
  for (const event of deduped) {
    const resolution = parseUserInputResolution(event);
    const requestId = getResolvedRequestId(event);
    if (resolution && requestId) resolved.set(requestId, resolution.outcome);
  }
  return deduped
    .filter((event) => event.kind === KIND_AGENT_USER_INPUT_REQUESTED)
    .map((event) => {
      const request = parseUserInputRequest(event);
      return request ? { event, request } : null;
    })
    .filter((item): item is UserInputEvent => item !== null)
    .filter(
      ({ event }) =>
        !answered.has(event.id) &&
        !resolved.has(event.id) &&
        !optimisticallyResolvedIds.has(event.id),
    )
    .sort((left, right) => right.event.created_at - left.event.created_at);
}

export function deriveResolvedUserInputs(
  events: RelayEvent[],
): Array<UserInputEvent & { resolution: UserInputResolutionOutcome }> {
  const deduped = dedupeUserInputEvents(events);
  const resolutions = new Map<string, UserInputResolutionOutcome>();
  for (const event of deduped) {
    const requestId = getResolvedRequestId(event);
    const resolution = parseUserInputResolution(event);
    if (requestId && resolution) resolutions.set(requestId, resolution.outcome);
  }
  return deduped
    .filter((event) => event.kind === KIND_AGENT_USER_INPUT_REQUESTED)
    .map((event) => {
      const request = parseUserInputRequest(event);
      const outcome = resolutions.get(event.id);
      return request && outcome
        ? { event, request, resolution: outcome }
        : null;
    })
    .filter(
      (
        item,
      ): item is UserInputEvent & {
        resolution: UserInputResolutionOutcome;
      } => item !== null,
    )
    .sort((left, right) => right.event.created_at - left.event.created_at);
}

export function buildUserInputAnswers(
  values: UserInputAnswers,
): UserInputAnswers {
  return values;
}

export function serializeUserInputAnswers(values: UserInputAnswers): string {
  return JSON.stringify(values);
}

// `null` is decoded by the harness as `None`: optional fields are omitted,
// while a required field cancels the form. This intentionally means "answer
// nothing", rather than dismissing the durable request locally.
export function buildSkippedAnswers(questionIds: string[]): UserInputAnswers {
  return Object.fromEntries(questionIds.map((id) => [id, null]));
}
