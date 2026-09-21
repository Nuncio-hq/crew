import type { ThreadRecap, RecapStatus } from "../types";

export type RecapUiStatus = RecapStatus | "loading";

export type RecapState = {
  status: RecapUiStatus;
  requestId: string | null;
  cancelRequested: boolean;
  recap: ThreadRecap | null;
  error: string | null;
};

export function initialRecapState(): RecapState {
  return {
    status: "loading",
    requestId: null,
    cancelRequested: false,
    recap: null,
    error: null,
  };
}

export type RecapAction =
  | {
      type: "reset";
    }
  | {
      type: "availability";
      status: "off" | "unsupported" | "missing_selection";
      error?: string | null;
    }
  | {
      type: "unavailable";
      error?: string | null;
    }
  | {
      type: "loaded";
      status: "no_recap" | "current" | "stale";
      recap: ThreadRecap | null;
      error?: string | null;
    }
  | {
      type: "generate_started";
      requestId: string;
    }
  | {
      type: "generated";
      requestId: string;
      recap: ThreadRecap;
    }
  | {
      type: "generation_failed";
      requestId: string;
      error: string;
    }
  | {
      type: "cancel_requested";
      requestId: string;
    }
  | {
      type: "cancelled";
      requestId: string;
    };

/**
 * Keep recap completions tied to the request that created them. This reducer
 * is deliberately pure so stale provider results and cancel races can be
 * tested without invoking a runtime.
 */
export function recapReducer(
  state: RecapState,
  action: RecapAction,
): RecapState {
  switch (action.type) {
    case "reset":
      return initialRecapState();
    case "availability":
      return {
        ...state,
        status: action.status,
        requestId: null,
        cancelRequested: false,
        error: action.error ?? null,
      };
    case "unavailable":
      return {
        ...state,
        status: "unsupported",
        requestId: null,
        cancelRequested: false,
        error: action.error ?? null,
      };
    case "loaded":
      return {
        ...state,
        status: action.status,
        requestId: null,
        cancelRequested: false,
        recap: action.recap,
        error: action.error ?? null,
      };
    case "generate_started":
      return {
        ...state,
        status: "generating",
        requestId: action.requestId,
        cancelRequested: false,
        error: null,
      };
    case "generated":
      if (state.requestId !== action.requestId || state.cancelRequested) {
        return state;
      }
      return {
        ...state,
        status: "current",
        requestId: null,
        cancelRequested: false,
        recap: action.recap,
        error: null,
      };
    case "generation_failed":
      if (state.requestId !== action.requestId) return state;
      return {
        ...state,
        status: "failed",
        requestId: null,
        cancelRequested: false,
        error: action.error,
      };
    case "cancel_requested":
      if (state.requestId !== action.requestId) return state;
      return {
        ...state,
        cancelRequested: true,
        error: null,
      };
    case "cancelled":
      if (state.requestId !== action.requestId) return state;
      return {
        ...state,
        status: "cancelled",
        requestId: null,
        cancelRequested: false,
        error: null,
      };
  }
}

/** User-facing text for the typed error codes returned by recap commands. */
const RECAP_ERROR_MESSAGES: Record<string, string> = {
  cancelled: "Recap generation was cancelled.",
  capability_changed:
    "The certified runtime changed since settings were saved. Re-save recap settings.",
  generation_in_progress: "A recap is already being generated for this thread.",
  generation_limit: "Too many recaps are generating; wait for one to finish.",
  generation_mismatch: "The running recap no longer matches this request.",
  generation_not_found: "No running recap matches this thread anymore.",
  invalid_generation: "The recap request was not accepted.",
  invalid_model_selection:
    "The selected model is not certified for this runtime.",
  invalid_relay: "This relay does not support recap requests.",
  invalid_settings:
    "Recap settings could not be read; re-save them in Settings.",
  invalid_thread: "This thread cannot be recapped.",
  missing_selection:
    "Choose a supported runtime in Settings before generating.",
  output_limit: "The runtime returned an oversized recap; it was discarded.",
  profile_mismatch: "The selected profile is not certified for this runtime.",
  recap_off: "Recap is Off. Enable manual generation in Settings first.",
  recap_task_failed: "The recap worker could not complete; nothing was saved.",
  runtime_not_ready:
    "A certified recap runtime is unavailable on this installation.",
  settings_unavailable:
    "Saved recap settings are unreadable; save a fresh selection to repair.",
  source_overflow:
    "The recap covers a bounded window; earlier or newer replies may be missing.",
  source_timeout: "Reading the thread history timed out.",
  source_unavailable:
    "The thread source could not be read under current access.",
  state_io: "The local recap cache could not be written.",
  state_ownership: "Recap state ownership could not be verified.",
};

export function recapErrorMessage(error: unknown): string {
  const raw =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : "";
  const code = raw.trim();
  if (code in RECAP_ERROR_MESSAGES) return RECAP_ERROR_MESSAGES[code];
  if (code) return raw;
  return "The recap service could not complete this request.";
}
