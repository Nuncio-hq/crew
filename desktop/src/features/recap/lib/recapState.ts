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

export function recapErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "The recap service could not complete this request.";
}
