import * as React from "react";

import { buildMessageLink } from "@/features/messages/lib/messageLink";

import { recapClient as defaultRecapClient, type RecapClient } from "../api";
import {
  hasValidRecapSelection,
  recapRuntimeLabel,
  selectedRuntime,
  type RecapSettingsSnapshot,
  type RecapStatus,
  type ThreadRecapRequest,
} from "../types";
import {
  initialRecapState,
  recapErrorMessage,
  recapReducer,
  type RecapState,
} from "../lib/recapState";

export type ThreadRecapPanelProps = ThreadRecapRequest & {
  /** Local projection watermark used to re-read backend freshness after edits. */
  sourceRevision?: string;
  client?: RecapClient;
  /** Optional in-app navigation hook; the href remains usable as a deep link. */
  onOpenSource?: (eventId: string) => void;
};

export function recapStatusLabel(status: RecapStatus | "loading"): string {
  switch (status) {
    case "loading":
      return "Checking runtime support…";
    case "off":
      return "Off";
    case "unsupported":
      return "Unavailable";
    case "missing_selection":
      return "Setup needed";
    case "no_recap":
      return "No recap yet";
    case "generating":
      return "Generating";
    case "current":
      return "Current";
    case "stale":
      return "Out of date";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
  }
}

export function recapActionLabel(
  state: Pick<RecapState, "status" | "recap">,
): string {
  if (state.status === "generating") return "Generating…";
  return state.recap ? "Regenerate recap" : "Generate recap";
}

function formatGeneratedAt(timestamp: number): string {
  if (!Number.isFinite(timestamp)) return "unknown time";
  return new Date(timestamp).toLocaleString();
}

function initialStatusForSettings(
  snapshot: RecapSettingsSnapshot,
): "off" | "unsupported" | "missing_selection" | null {
  if (snapshot.settings.mode === "off") return "off";
  const runtime = selectedRuntime(snapshot.settings, snapshot.runtimes);
  if (!runtime) return "missing_selection";
  if (runtime.availability !== "supported") return "unsupported";
  if (!hasValidRecapSelection(snapshot.settings, snapshot.runtimes)) {
    return "missing_selection";
  }
  return null;
}

export function ThreadRecapPanel({
  channelId,
  rootEventId,
  sourceRevision = "",
  client = defaultRecapClient,
  onOpenSource,
}: ThreadRecapPanelProps) {
  const [state, dispatch] = React.useReducer(
    recapReducer,
    undefined,
    initialRecapState,
  );
  const [settingsSnapshot, setSettingsSnapshot] =
    React.useState<RecapSettingsSnapshot | null>(null);
  const requestSequence = React.useRef(0);

  React.useEffect(() => {
    // The revision is intentionally a dependency: edits trigger a fresh
    // backend freshness read even though the command only needs the IDs.
    void sourceRevision;
    let active = true;
    setSettingsSnapshot(null);
    dispatch({ type: "reset" });

    void client
      .getSettings()
      .then((snapshot) => {
        if (!active) return;
        setSettingsSnapshot(snapshot);
        const availability = initialStatusForSettings(snapshot);
        if (availability) {
          dispatch({ type: "availability", status: availability });
          return;
        }
        void client
          .getThreadRecap({ channelId, rootEventId })
          .then((lookup) => {
            if (!active) return;
            dispatch({
              type: "loaded",
              status: lookup.status,
              recap: lookup.recap,
              error: lookup.reason ?? null,
            });
          })
          .catch((error: unknown) => {
            if (!active) return;
            dispatch({
              type: "loaded",
              status: "no_recap",
              recap: null,
              error: recapErrorMessage(error),
            });
          });
      })
      .catch((error: unknown) => {
        if (!active) return;
        dispatch({ type: "unavailable", error: recapErrorMessage(error) });
      });

    return () => {
      active = false;
    };
  }, [channelId, client, rootEventId, sourceRevision]);

  const settings = settingsSnapshot?.settings ?? null;
  const canGenerate = Boolean(
    settings &&
      settings.mode === "manual" &&
      hasValidRecapSelection(settings, settingsSnapshot?.runtimes ?? []),
  );
  const isGenerating = state.status === "generating";
  const isCancelling = isGenerating && state.cancelRequested;
  const request = React.useCallback(() => {
    if (!canGenerate || isGenerating) return;
    const generationId = `recap-${Date.now()}-${requestSequence.current + 1}`;
    requestSequence.current += 1;
    dispatch({ type: "generate_started", requestId: generationId });
    void client
      .generateThreadRecap({ channelId, rootEventId, generationId })
      .then((recap) => {
        dispatch({ type: "generated", requestId: generationId, recap });
      })
      .catch((error: unknown) => {
        dispatch({
          type: "generation_failed",
          requestId: generationId,
          error: recapErrorMessage(error),
        });
      });
  }, [canGenerate, channelId, client, isGenerating, rootEventId]);

  const cancel = React.useCallback(() => {
    const generationId = state.requestId;
    if (!generationId || state.cancelRequested) return;
    dispatch({ type: "cancel_requested", requestId: generationId });
    void client
      .cancelThreadRecap({ channelId, rootEventId, generationId })
      .then(() => {
        dispatch({ type: "cancelled", requestId: generationId });
      })
      .catch((error: unknown) => {
        dispatch({
          type: "generation_failed",
          requestId: generationId,
          error: recapErrorMessage(error),
        });
      });
  }, [channelId, client, rootEventId, state.cancelRequested, state.requestId]);

  const statusMessage = (() => {
    switch (state.status) {
      case "loading":
        return "Checking whether a certified recap runtime is available…";
      case "off":
        return "Recap is Off in Settings.";
      case "unsupported":
        return "A certified recap runtime is unavailable. Recap remains Off.";
      case "missing_selection":
        return "Choose a supported runtime and profile in Settings before generating.";
      case "no_recap":
        return "No recap generated for this thread.";
      case "generating":
        return isCancelling
          ? "Cancelling recap and cleaning up its owned run…"
          : "Generating a local recap from the accessible thread history…";
      case "current":
        return "Generated from the recorded thread source.";
      case "stale":
        return "Thread history changed. Regenerate to refresh this recap.";
      case "failed":
        return "The previous recap is retained; fix the issue and retry.";
      case "cancelled":
        return "Recap generation was cancelled.";
    }
  })();

  return (
    <section
      aria-label="Thread recap"
      className="space-y-2 border-t border-border/60 pt-3"
      data-testid="thread-recap"
    >
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-semibold">Recap</h3>
        <span
          className="text-xs text-muted-foreground"
          data-testid="thread-recap-status"
        >
          {recapStatusLabel(state.status)}
        </span>
      </div>

      {state.recap ? (
        <div
          className="space-y-2 rounded-md border border-border/60 bg-muted/20 p-2.5"
          data-testid="thread-recap-result"
        >
          <p className="whitespace-pre-wrap text-sm">{state.recap.text}</p>
          <p className="text-xs text-muted-foreground" data-settings-subcopy>
            {state.recap.provenance === "verified"
              ? "Verified"
              : "Requested only"}{" "}
            ·{" "}
            {recapRuntimeLabel(
              {
                runtimeId: state.recap.runtimeId,
                requestedModel: state.recap.requestedModel,
                profileRef: state.recap.profileRef,
              },
              settingsSnapshot?.runtimes ?? [],
            )}
            {state.recap.effectiveModel
              ? ` · ${state.recap.effectiveModel}`
              : ""}{" "}
            · {formatGeneratedAt(state.recap.generatedAt)}
          </p>
          <p className="text-xs text-muted-foreground" data-settings-subcopy>
            {state.recap.sourceEventIds.length} source message
            {state.recap.sourceEventIds.length === 1 ? "" : "s"}
            {state.recap.omittedMessageCount > 0
              ? ` · ${state.recap.omittedMessageCount} omitted by the source bound`
              : ""}
          </p>
          {state.recap.sourceEventIds.length > 0 ? (
            <fieldset className="flex flex-wrap gap-x-2 gap-y-1 text-xs">
              <legend className="sr-only">Recap sources</legend>
              {state.recap.sourceEventIds.map((eventId, index) => (
                <a
                  className="text-primary underline-offset-2 hover:underline"
                  data-testid="thread-recap-source-link"
                  href={buildMessageLink({
                    channelId,
                    messageId: eventId,
                    threadRootId: rootEventId,
                  })}
                  key={eventId}
                  onClick={(event) => {
                    if (!onOpenSource) return;
                    event.preventDefault();
                    onOpenSource(eventId);
                  }}
                >
                  Source {index + 1}
                </a>
              ))}
            </fieldset>
          ) : null}
        </div>
      ) : null}

      <p
        className="text-xs text-muted-foreground/80"
        data-testid="thread-recap-status-message"
        role={state.error ? "alert" : "status"}
      >
        {state.error ?? statusMessage}
      </p>

      <div className="flex flex-wrap items-center gap-2">
        <button
          className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
          data-testid="thread-recap-generate"
          disabled={!canGenerate || isGenerating}
          onClick={request}
          type="button"
        >
          {recapActionLabel(state)}
        </button>
        {isGenerating ? (
          <button
            className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
            data-testid="thread-recap-cancel"
            disabled={isCancelling}
            onClick={cancel}
            type="button"
          >
            {isCancelling ? "Cancelling…" : "Cancel"}
          </button>
        ) : null}
      </div>
    </section>
  );
}
