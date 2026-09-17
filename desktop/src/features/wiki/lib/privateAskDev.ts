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
  refusal: string | null;
  citations: PrivateAskCitation[];
  askedAt: number;
};

/**
 * What `private_ask_run` returns. The citations are the ones the ANSWER cited,
 * resolved by the backend against the verified snapshot — not a copy of the
 * grounding it was given.
 */
export type PrivateAskRunResult = {
  attemptId: string;
  markdown: string;
  citations: PrivateAskCitation[];
  /** False when the answer was produced but could not be kept on this machine. */
  historyRecorded: boolean;
};

export type PrivateAskState = {
  question: string;
  /** Streamed-or-complete: partial text is shown as it arrives. */
  answer: string;
  citations: PrivateAskCitation[];
  error: string | null;
  running: boolean;
  /**
   * Owner-local, read back from the backend on mount. It is the machine's own
   * record: a private Ask publishes nothing, so nothing else holds it.
   */
  history: PrivateAskHistoryEntry[];
  /** False when the last answer could not be written to that record. */
  historyRecorded: boolean;
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
  historyRecorded: true,
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
      /** Whether the backend kept this attempt. Defaults to kept. */
      historyRecorded?: boolean;
    }
  | { type: "refused"; attemptId: string; error: string }
  | { type: "cancelled" }
  | { type: "restored"; history: PrivateAskHistoryEntry[] };

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
        historyRecorded: action.historyRecorded ?? true,
        history: remember(state.history, {
          attemptId: action.attemptId,
          question: state.question,
          markdown: action.markdown,
          refusal: null,
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
        historyRecorded: true,
        history: remember(state.history, {
          attemptId: action.attemptId,
          question: state.question,
          markdown: null,
          refusal: action.error,
          citations: [],
          askedAt: Date.now(),
        }),
      };
    case "restored":
      // The backend's record replaces whatever this window accumulated: it is
      // the one that survived a restart, and it is already bounded and pruned
      // there. A merge would resurrect entries the backend's own age bound
      // dropped.
      return { ...state, history: action.history };
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
