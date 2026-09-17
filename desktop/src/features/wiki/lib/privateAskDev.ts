/**
 * Owner-local state for the developer-only private Ask composer (#365).
 *
 * The logic lives here, apart from the component, so every transition is
 * testable without a DOM. The component below it only renders what this
 * returns.
 *
 * Nothing here decides whether the surface exists — the backend does, via
 * `private_ask_dev_status`. A renderer-side flag would not be a gate: the
 * webview can invoke any registered command, so the command bodies hold the
 * real one.
 */

export type PrivateAskDevStatus = {
  enabled: boolean;
  blockedReason: string | null;
};

export type PrivateAskCitation = {
  path: string;
  startLine: number;
  endLine: number;
};

/**
 * One finished attempt, kept on this machine only. A private Ask publishes
 * nothing, so this history has no remote counterpart to reconcile with.
 */
export type PrivateAskHistoryEntry = {
  attemptId: string;
  question: string;
  /** Present when the attempt produced an answer. */
  markdown: string | null;
  /** Present when the attempt was refused; mutually exclusive with markdown. */
  error: string | null;
  citations: PrivateAskCitation[];
  askedAt: number;
};

export type PrivateAskState = {
  question: string;
  /** Streamed-or-complete: partial text is shown as it arrives. */
  answer: string;
  citations: PrivateAskCitation[];
  error: string | null;
  running: boolean;
  history: PrivateAskHistoryEntry[];
};

/**
 * Retention bound for the owner-local history, matching the recap retention
 * pattern: keep a bounded, newest-first window rather than growing without
 * limit on a machine nobody prunes.
 */
export const PRIVATE_ASK_HISTORY_LIMIT = 20;

export const initialPrivateAskState: PrivateAskState = {
  question: "",
  answer: "",
  citations: [],
  error: null,
  running: false,
  history: [],
};

export type PrivateAskAction =
  | { type: "question"; value: string }
  | { type: "start" }
  | { type: "chunk"; value: string }
  | {
      type: "answered";
      attemptId: string;
      markdown: string;
      citations: PrivateAskCitation[];
    }
  | { type: "refused"; attemptId: string; error: string }
  | { type: "cancelled" };

/** Whether the Ask control should be actionable right now. */
export function canAskPrivately(
  state: PrivateAskState,
  status: PrivateAskDevStatus,
): boolean {
  return status.enabled && !state.running && state.question.trim().length > 0;
}

function remember(
  history: PrivateAskHistoryEntry[],
  entry: PrivateAskHistoryEntry,
): PrivateAskHistoryEntry[] {
  // Newest first, bounded. The slice is what keeps a long-lived install from
  // accumulating every question ever asked.
  return [entry, ...history].slice(0, PRIVATE_ASK_HISTORY_LIMIT);
}

export function privateAskReducer(
  state: PrivateAskState,
  action: PrivateAskAction,
): PrivateAskState {
  switch (action.type) {
    case "question":
      return { ...state, question: action.value };
    case "start":
      // Clear the previous answer: leaving it on screen beside a new spinner
      // reads as though the new question was already answered.
      return {
        ...state,
        running: true,
        answer: "",
        citations: [],
        error: null,
      };
    case "chunk":
      // Ignored unless a run is in flight, so a late chunk from a cancelled
      // attempt cannot paint itself over an idle composer.
      return state.running
        ? { ...state, answer: state.answer + action.value }
        : state;
    case "answered":
      return {
        ...state,
        running: false,
        answer: action.markdown,
        citations: action.citations,
        error: null,
        history: remember(state.history, {
          attemptId: action.attemptId,
          question: state.question,
          markdown: action.markdown,
          error: null,
          citations: action.citations,
          askedAt: Date.now(),
        }),
      };
    case "refused":
      // A refusal is history too. The feature is currently refused end to end,
      // and hiding that would make an intentional fence look like a hang.
      return {
        ...state,
        running: false,
        answer: "",
        citations: [],
        error: action.error,
        history: remember(state.history, {
          attemptId: action.attemptId,
          question: state.question,
          markdown: null,
          error: action.error,
          citations: [],
          askedAt: Date.now(),
        }),
      };
    case "cancelled":
      // Cancelling records nothing: the viewer withdrew the question, so there
      // is no outcome worth keeping.
      return {
        ...state,
        running: false,
        answer: "",
        citations: [],
        error: null,
      };
    default:
      return state;
  }
}
