/**
 * Owner-local state for the developer-only private Ask composer (#365/#366).
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

/**
 * One agent a private Ask may be addressed to. `status` is the resolver's
 * own reading of the live runtime — `busy` and `offline` agents are shown
 * as unavailable with a recovery choice, never silently substituted.
 */
export type PrivateAskAgent = {
  pubkey: string;
  name: string;
  /** The relay the harness is bound to; an offline agent's start needs it. */
  relayUrl: string;
  /** `ready` | `busy` | `offline` — what the resolver would conclude now. */
  status: "ready" | "busy" | "offline" | string;
};

export type PrivateAskCitation = {
  path: string;
  startLine: number;
  endLine: number;
};

/** One page in the coverage record: which page, and why it was taken. */
export type PrivateAskManifestPage = {
  slug: string;
  title: string;
  score: number;
};

/** One source range in the coverage record: where it lives, nothing more. */
export type PrivateAskManifestSource = {
  path: string;
  startLine: number;
  endLine: number;
};

/**
 * The retrieval record attached to an outcome: what the prompt consulted and
 * what the bounds omitted. It is how a viewer sees the "not enough source"
 * boundary rather than being told to trust the answer.
 */
export type PrivateAskManifest = {
  includedPages: PrivateAskManifestPage[];
  omittedPages: PrivateAskManifestPage[];
  includedSources: PrivateAskManifestSource[];
  omittedSources: PrivateAskManifestSource[];
  sourceGrant: boolean;
};

/**
 * One finished or in-flight attempt, kept on this machine only. A private Ask
 * publishes nothing, so this history has no remote counterpart to reconcile
 * with.
 */
export type PrivateAskHistoryEntry = {
  attemptId: string;
  questionId: string;
  /** The attempt this question followed up on, when it is one. */
  followUpOf: string | null;
  /** `running` | `interrupted` | `answered` | `insufficient` | `cancelled` |
   * `refused` — the stored status resolved against the live registry. */
  status:
    | "running"
    | "interrupted"
    | "answered"
    | "insufficient"
    | "cancelled"
    | "refused";
  /** The typed refusal detail, in the viewer-safe vocabulary. */
  detail: string | null;
  question: string;
  markdown: string | null;
  citations: PrivateAskCitation[];
  manifest: PrivateAskManifest | null;
  sourceRevision: string | null;
  /** Who answered — a stable pubkey, not a name that could have changed. */
  agentPubkey: string;
  askedAt: number;
  finishedAt: number | null;
};

/** What `private_ask_run` returns. */
export type PrivateAskRunResult = {
  /** The backend's own attempt id — the fence late stream events match. */
  attemptId: string;
  /** The question thread this attempt landed on. */
  questionId: string;
  status: "answered" | "insufficient" | "cancelled" | "refused" | string;
  markdown: string;
  /** The typed refusal, when the status is `refused`. */
  refusal: string | null;
  citations: PrivateAskCitation[];
  sourceRevision: string | null;
  manifest: PrivateAskManifest | null;
  /** False when the outcome could not be kept on this machine. */
  historyRecorded: boolean;
};

/**
 * The read-only draft input #367 consumes (#366 step 8). Producing this shape
 * never sends anything — it is the validated material a draft dispatch would
 * start from: the exact question/attempt, the answer as validated, the source
 * manifest, and where it came from.
 */
export type PrivateAskDraftInput = {
  questionId: string;
  attemptId: string;
  question: string;
  markdown: string;
  citations: PrivateAskCitation[];
  manifest: PrivateAskManifest | null;
  sourceRevision: string | null;
  originCoordinate: string;
  originAgent: string;
};

/**
 * The lifecycle an attempt moves through. `interrupted` is what a `running`
 * record becomes when the process that owned it is gone — it is a truthful
 * state, not a retry prompt: a run whose fate is unknown is never silently
 * retried.
 */
export type PrivateAskPhase =
  | "idle"
  | "retrieving"
  | "running"
  | "answered"
  | "insufficient"
  | "refused"
  | "cancelled"
  | "interrupted";

export type PrivateAskState = {
  phase: PrivateAskPhase;
  question: string;
  /**
   * The attempt the composer is fenced to. Stream events and the settle
   * result are only honoured while they carry this id — a late chunk or a
   * late completion from a cancelled or superseded attempt cannot paint
   * itself over the current state.
   */
  attemptId: string | null;
  /** The question thread the active attempt belongs to. */
  questionId: string | null;
  /**
   * The answered attempt the next question follows up on, when the viewer
   * chose to follow up. Cleared by an explicit detach, never by guessing.
   */
  followUpOf: string | null;
  /** Streamed-or-complete: partial text is shown as it arrives. */
  answer: string;
  citations: PrivateAskCitation[];
  manifest: PrivateAskManifest | null;
  sourceRevision: string | null;
  error: string | null;
  /** False when the last outcome — answer or refusal — could not be written
   * to the owner-local record. */
  historyRecorded: boolean;
  /** Owner-local, read back from the backend on mount. */
  history: PrivateAskHistoryEntry[];
};

export const initialPrivateAskState: PrivateAskState = {
  phase: "idle",
  question: "",
  attemptId: null,
  questionId: null,
  followUpOf: null,
  answer: "",
  citations: [],
  manifest: null,
  sourceRevision: null,
  error: null,
  historyRecorded: true,
  history: [],
};

export type PrivateAskAction =
  | { type: "question"; value: string }
  | {
      type: "begin";
      attemptId: string;
      questionId: string;
      followUpOf: string | null;
    }
  | {
      type: "phase";
      attemptId: string;
      phase: "retrieving" | "running";
    }
  | { type: "chunk"; attemptId: string; text: string }
  | { type: "settled"; result: PrivateAskRunResult }
  | { type: "rejected"; attemptId: string; error: string }
  | { type: "cancelled"; attemptId: string }
  | { type: "opened"; entry: PrivateAskHistoryEntry }
  | { type: "detachFollowUp" }
  | { type: "dismissed" }
  | { type: "restored"; history: PrivateAskHistoryEntry[] };

/** Whether the Ask control should be actionable right now. */
export function canAskPrivately(
  state: PrivateAskState,
  status: PrivateAskDevStatus,
  agentId: string,
): boolean {
  return (
    status.enabled &&
    agentId.length > 0 &&
    (state.phase === "idle" ||
      state.phase === "answered" ||
      state.phase === "insufficient" ||
      state.phase === "refused" ||
      state.phase === "cancelled" ||
      state.phase === "interrupted") &&
    state.question.trim().length > 0
  );
}

/** Whether a Stop control replaces Ask — an in-flight attempt exists. */
export function isAskInFlight(state: PrivateAskState): boolean {
  return (
    state.attemptId !== null &&
    (state.phase === "retrieving" || state.phase === "running")
  );
}

/** Whether the outcome view is a terminal state Retry makes sense for. */
export function canRetryAsk(state: PrivateAskState): boolean {
  return (
    state.attemptId === null &&
    state.question.trim().length > 0 &&
    (state.phase === "refused" ||
      state.phase === "cancelled" ||
      state.phase === "interrupted" ||
      state.phase === "insufficient")
  );
}

/** The status a `running` history record maps to when nothing is in flight. */
function truthfulStoredPhase(entry: PrivateAskHistoryEntry): PrivateAskPhase {
  // The backend resolves `running` against the live registry before this ever
  // reaches the reducer, so a `running` entry here really is owned by a live
  // attempt — adopted so Stop can still name it after navigation.
  return entry.status;
}

export function privateAskReducer(
  state: PrivateAskState,
  action: PrivateAskAction,
): PrivateAskState {
  switch (action.type) {
    case "question":
      return { ...state, question: action.value };
    case "begin":
      // A fresh attempt replaces whatever was on screen: leaving the last
      // answer beside a new spinner reads as though the new question was
      // already answered. The question thread keeps its id; a follow-up names
      // the attempt it builds on.
      return {
        ...state,
        phase: "retrieving",
        attemptId: action.attemptId,
        questionId: action.questionId,
        followUpOf: action.followUpOf,
        answer: "",
        citations: [],
        manifest: null,
        sourceRevision: null,
        error: null,
        historyRecorded: true,
      };
    case "phase":
      // Progress is the attempt's own report: an event for a superseded or
      // foreign attempt is dropped, and only a forward move is honoured —
      // `retrieving` after `running` would be a lie about where the run is.
      if (action.attemptId !== state.attemptId) {
        return state;
      }
      if (state.phase === "retrieving" || state.phase === "running") {
        return { ...state, phase: action.phase };
      }
      return state;
    case "chunk":
      if (
        action.attemptId !== state.attemptId ||
        (state.phase !== "running" && state.phase !== "retrieving")
      ) {
        return state;
      }
      return { ...state, answer: state.answer + action.text };
    case "settled": {
      const { result } = action;
      // The fence: a settle for an attempt this composer is not watching is a
      // late completion. It is still the viewer's record — history is
      // refreshed by the caller — but it must not overwrite the active pane.
      if (result.attemptId !== state.attemptId) {
        return state;
      }
      const answered = result.status === "answered";
      const followUpOf = answered ? result.attemptId : state.followUpOf;
      return {
        ...state,
        phase:
          result.status === "answered"
            ? "answered"
            : result.status === "insufficient"
              ? "insufficient"
              : result.status === "cancelled"
                ? "cancelled"
                : "refused",
        attemptId: null,
        questionId: result.questionId,
        followUpOf,
        answer: result.markdown,
        citations: result.citations,
        manifest: result.manifest,
        sourceRevision: result.sourceRevision,
        error: result.refusal,
        historyRecorded: result.historyRecorded,
      };
    }
    case "rejected":
      // The command itself could not run — a closed gate, a foreign id, a
      // blank question. The attempt never started; the refusal is shown
      // without claiming anything about the machine's record.
      if (action.attemptId !== state.attemptId) {
        return state;
      }
      return {
        ...state,
        phase: "refused",
        attemptId: null,
        error: action.error,
      };
    case "cancelled":
      // The viewer withdrew the question; the backend keeps the cancelled
      // record. The question text stays so Retry is one tap.
      if (action.attemptId !== state.attemptId) {
        return state;
      }
      return {
        ...state,
        phase: "cancelled",
        attemptId: null,
        answer: "",
        citations: [],
        manifest: null,
        sourceRevision: null,
        error: null,
      };
    case "opened":
      // Reopen an attempt's record into the pane — answer, citations and
      // coverage exactly as kept, with the question it answered. A `running`
      // record re-attaches its attempt id so Stop still names the owned run.
      return {
        ...state,
        phase: truthfulStoredPhase(action.entry),
        attemptId:
          action.entry.status === "running" ? action.entry.attemptId : null,
        questionId: action.entry.questionId,
        followUpOf:
          action.entry.status === "answered" ? action.entry.attemptId : null,
        question: action.entry.question,
        answer: action.entry.markdown ?? "",
        citations: action.entry.citations,
        manifest: action.entry.manifest,
        sourceRevision: action.entry.sourceRevision,
        error: action.entry.detail,
        historyRecorded: true,
      };
    case "detachFollowUp":
      return { ...state, followUpOf: null };
    case "dismissed":
      return {
        ...state,
        phase: "idle",
        answer: "",
        citations: [],
        manifest: null,
        sourceRevision: null,
        error: null,
        historyRecorded: true,
      };
    case "restored":
      // The backend's record replaces whatever this window accumulated: it is
      // the one that survived a restart, and it is already bounded and pruned
      // there. A merge would resurrect entries the backend's own age bound
      // dropped.
      return { ...state, history: action.history };
    default:
      return state;
  }
}

/**
 * The localStorage key the per-project picker memory lives under, scoped to
 * the repository coordinate — a preference, not conversation state, and never
 * a globally keyed record (private history itself is backend-owned).
 */
export function rememberedAgentKey(coordinate: string): string {
  return `crew.private-ask.agent.${coordinate}`;
}

/**
 * Subscribe to the attempt-scoped progress events the backend emits. The
 * returned function unsubscribes both listeners; a backend that cannot
 * register listeners rejects, and the caller treats that as "no progress
 * feed" rather than a failure of the surface itself.
 */
export async function subscribePrivateAskEvents(handlers: {
  onProgress(attemptId: string, phase: "retrieving" | "running"): void;
  onChunk(attemptId: string, text: string): void;
}): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlistens = await Promise.all([
    listen<{ attemptId: string; phase: "retrieving" | "running" }>(
      "private-ask:progress",
      (event) =>
        handlers.onProgress(event.payload.attemptId, event.payload.phase),
    ),
    listen<{ attemptId: string; text: string }>("private-ask:chunk", (event) =>
      handlers.onChunk(event.payload.attemptId, event.payload.text),
    ),
  ]);
  return () => {
    for (const unlisten of unlistens) {
      unlisten();
    }
  };
}
