import * as React from "react";

import {
  WIKI_ASK_UNAVAILABLE_REASON,
  type AskMode,
} from "@/features/wiki/lib/wikiAsk";
import {
  canAskPrivately,
  initialPrivateAskState,
  privateAskReducer,
  type PrivateAskDevStatus,
} from "@/features/wiki/lib/privateAskDev";
import { invokeTauri } from "@/shared/api/tauri";
import {
  OFFICE_COMPOSER_SURFACE_CLASS,
  OFFICE_FIELD_BOX_CLASS,
  OFFICE_FIELD_CONTROL_CLASS,
  OFFICE_SURFACE,
} from "@/shared/layout/officeChrome";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

const CLOSED_STATUS: PrivateAskDevStatus = {
  enabled: false,
  blockedReason: null,
};

/**
 * Ask the backend whether the developer surface exists. The renderer never
 * decides this: a webview can invoke any registered command, so the gate lives
 * in the command bodies and this only mirrors their answer.
 */
function usePrivateAskDevStatus(): PrivateAskDevStatus {
  const [status, setStatus] =
    React.useState<PrivateAskDevStatus>(CLOSED_STATUS);

  React.useEffect(() => {
    let active = true;
    invokeTauri<PrivateAskDevStatus>("private_ask_dev_status")
      .then((next) => {
        if (active) {
          setStatus(next);
        }
      })
      .catch(() => {
        // A backend that cannot answer is a backend that has not enabled this.
        // Staying closed is the safe reading, and the user placeholder below
        // already explains the state.
        if (active) {
          setStatus(CLOSED_STATUS);
        }
      });
    return () => {
      active = false;
    };
  }, []);

  return status;
}

export function WikiAskBox({
  scopeLabel,
}: {
  channelId?: string | null;
  door: "library" | "project";
  owner?: string;
  repoD?: string;
  scopeLabel: string;
}) {
  const [mode, setMode] = React.useState<AskMode>("auto");
  const [question, setQuestion] = React.useState("");
  const status = usePrivateAskDevStatus();

  if (status.enabled) {
    return <PrivateAskDevComposer scopeLabel={scopeLabel} status={status} />;
  }

  return (
    <div className="shrink-0 px-4 pb-3 pt-2">
      <div
        className={OFFICE_COMPOSER_SURFACE_CLASS}
        data-office-surface={OFFICE_SURFACE.composerSurface}
        data-testid="wiki-ask"
      >
        <div className="mb-1 text-2xs text-muted-foreground">{scopeLabel}</div>
        <p
          aria-live="polite"
          className="mb-2 rounded-md border border-border bg-muted/20 p-2 text-2xs text-muted-foreground"
          data-testid="wiki-ask-unavailable"
          id="wiki-ask-unavailable"
          role="status"
        >
          {WIKI_ASK_UNAVAILABLE_REASON}
        </p>
        <div className="flex items-center gap-2">
          <select
            aria-label="Ask mode"
            aria-describedby="wiki-ask-unavailable"
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "h-8 px-2 text-2xs",
            )}
            data-testid="wiki-ask-mode"
            onChange={(event) => setMode(event.target.value as AskMode)}
            value={mode}
          >
            <option value="auto">Auto</option>
            <option value="qa">Q&A</option>
            <option value="plan">Plan</option>
          </select>
          <input
            aria-label="Ask the wiki"
            aria-describedby="wiki-ask-unavailable"
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "h-8 min-w-0 flex-1 px-2 text-sm",
            )}
            data-testid="wiki-ask-input"
            onChange={(event) => setQuestion(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || event.nativeEvent.isComposing) {
                return;
              }
              event.preventDefault();
            }}
            placeholder="Ask about this wiki"
            value={question}
          />
          <Button
            aria-describedby="wiki-ask-unavailable"
            disabled
            size="sm"
            type="button"
          >
            Ask
          </Button>
        </div>
      </div>
    </div>
  );
}

/**
 * The developer-only composer. It is deliberately bare: no mode selector, no
 * persistence beyond this window, and it shows the backend's refusal verbatim
 * rather than dressing it up, because the refusal is the thing under test.
 */
function PrivateAskDevComposer({
  scopeLabel,
  status,
}: {
  scopeLabel: string;
  status: PrivateAskDevStatus;
}) {
  const [state, dispatch] = React.useReducer(
    privateAskReducer,
    initialPrivateAskState,
  );
  const attempt = React.useRef(0);

  const ask = React.useCallback(() => {
    if (!canAskPrivately(state, status)) {
      return;
    }
    attempt.current += 1;
    const attemptId = `attempt-${attempt.current}`;
    dispatch({ type: "start" });
    invokeTauri<string>("private_ask_run", { question: state.question })
      .then((markdown) => {
        dispatch({ type: "answered", attemptId, markdown, citations: [] });
      })
      .catch((error: unknown) => {
        dispatch({
          type: "refused",
          attemptId,
          error: error instanceof Error ? error.message : String(error),
        });
      });
  }, [state, status]);

  const cancel = React.useCallback(() => {
    dispatch({ type: "cancelled" });
    // A cancel the backend never heard of is a no-op there, so a failure here
    // is not worth surfacing to a developer who already sees an idle composer.
    void invokeTauri("private_ask_cancel", { attemptId: null }).catch(() => {});
  }, []);

  return (
    <div className="shrink-0 px-4 pb-3 pt-2">
      <div
        className={OFFICE_COMPOSER_SURFACE_CLASS}
        data-office-surface={OFFICE_SURFACE.composerSurface}
        data-testid="wiki-ask-dev"
      >
        <div className="mb-1 text-2xs text-muted-foreground">
          {scopeLabel} · private Ask (developer build)
        </div>
        {status.blockedReason ? (
          <p
            aria-live="polite"
            className="mb-2 rounded-md border border-border bg-muted/20 p-2 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-blocked"
            role="status"
          >
            {status.blockedReason}
          </p>
        ) : null}
        <div className="flex items-center gap-2">
          <input
            aria-label="Ask this agent privately"
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "h-8 min-w-0 flex-1 px-2 text-sm",
            )}
            data-testid="wiki-ask-dev-input"
            onChange={(event) =>
              dispatch({ type: "question", value: event.target.value })
            }
            onKeyDown={(event) => {
              if (event.key !== "Enter" || event.nativeEvent.isComposing) {
                return;
              }
              event.preventDefault();
              ask();
            }}
            placeholder="Ask this agent privately"
            value={state.question}
          />
          {state.running ? (
            <Button
              data-testid="wiki-ask-dev-cancel"
              onClick={cancel}
              size="sm"
              type="button"
              variant="secondary"
            >
              Cancel
            </Button>
          ) : (
            <Button
              data-testid="wiki-ask-dev-submit"
              disabled={!canAskPrivately(state, status)}
              onClick={ask}
              size="sm"
              type="button"
            >
              Ask
            </Button>
          )}
        </div>

        {state.error ? (
          <p
            aria-live="polite"
            className="mt-2 rounded-md border border-border bg-muted/20 p-2 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-error"
            role="status"
          >
            {state.error}
          </p>
        ) : null}

        {state.answer ? (
          <div className="mt-2 text-sm" data-testid="wiki-ask-dev-answer">
            {state.answer}
            {state.citations.length > 0 ? (
              <ul
                className="mt-1 text-2xs text-muted-foreground"
                data-testid="wiki-ask-dev-citations"
              >
                {state.citations.map((citation) => (
                  <li key={`${citation.path}:${citation.startLine}`}>
                    {citation.path}:{citation.startLine}-{citation.endLine}
                  </li>
                ))}
              </ul>
            ) : null}
          </div>
        ) : null}

        {state.history.length > 0 ? (
          <ol
            className="mt-2 space-y-1 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-history"
          >
            {state.history.map((entry) => (
              <li key={entry.attemptId}>
                {entry.question} — {entry.error ?? "answered"}
              </li>
            ))}
          </ol>
        ) : null}
      </div>
    </div>
  );
}
