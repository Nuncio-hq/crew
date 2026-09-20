import * as React from "react";

import {
  WIKI_ASK_UNAVAILABLE_REASON,
  type AskMode,
} from "@/features/wiki/lib/wikiAsk";
import {
  canAskPrivately,
  canRetryAsk,
  initialPrivateAskState,
  isAskInFlight,
  privateAskReducer,
  rememberedAgentKey,
  subscribePrivateAskEvents,
  type PrivateAskAgent,
  type PrivateAskCitation,
  type PrivateAskDevStatus,
  type PrivateAskHistoryEntry,
  type PrivateAskRunResult,
} from "@/features/wiki/lib/privateAskDev";
import { invokeTauri } from "@/shared/api/tauri";
import { startManagedAgentRuntime } from "@/shared/api/tauriManagedAgents";
import { WikiTaskDispatchPanel } from "@/features/wiki/ui/WikiTaskDispatchPanel";
import {
  peekWikiAskFocus,
  takeWikiAskFocus,
} from "@/features/wiki/lib/wikiTaskOrigin";
import type { PrivateAskDraftInput } from "@/features/wiki/lib/privateAskDev";
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
  onOpenSource,
  owner,
  projectId,
  repoD,
  scopeLabel,
}: {
  channelId?: string | null;
  door: "library" | "project";
  onOpenSource?: (citation: PrivateAskCitation) => void;
  owner?: string;
  /** Stable project route identity for draft scoping; "library" on the
   * company surface when no project route hosts the pane. */
  projectId?: string;
  repoD?: string;
  scopeLabel: string;
}) {
  const [mode, setMode] = React.useState<AskMode>("auto");
  const [question, setQuestion] = React.useState("");
  const status = usePrivateAskDevStatus();

  if (status.enabled) {
    return (
      <PrivateAskDevComposer
        coordinate={owner && repoD ? `${owner}:${repoD}` : null}
        onOpenSource={onOpenSource}
        projectId={projectId ?? "library"}
        scopeLabel={scopeLabel}
        status={status}
      />
    );
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
 * One citation, as a control rather than decoration.
 *
 * It opens the cited source when the host supplied an opener; otherwise it
 * renders the path as plain text rather than a control that goes nowhere.
 */
function CitationLink({
  citation,
  onOpen,
}: {
  citation: PrivateAskCitation;
  onOpen?: (citation: PrivateAskCitation) => void;
}) {
  const label = `${citation.path}:${citation.startLine}-${citation.endLine}`;
  if (!onOpen) {
    return (
      <span data-testid={`wiki-ask-dev-citation-${citation.path}`}>
        {label}
      </span>
    );
  }
  return (
    <button
      className="font-mono text-2xs text-primary"
      data-testid={`wiki-ask-dev-citation-${citation.path}`}
      onClick={() => onOpen(citation)}
      type="button"
    >
      {label}
    </button>
  );
}

/** What one agent's availability line says and whether it can be asked. */
function agentAvailabilityLabel(status: string): string | null {
  switch (status) {
    case "busy":
      return "busy";
    case "offline":
      return "offline";
    default:
      return null;
  }
}

/**
 * The coverage record under an outcome: what the prompt consulted and what
 * the bounds omitted. Insufficiency is visible here too — an empty included
 * list beside an omitted list is the honest "not enough source" boundary.
 */
function ManifestPanel({
  manifest,
}: {
  manifest: PrivateAskRunResult["manifest"];
}) {
  if (!manifest) {
    return null;
  }
  return (
    <div
      className="mt-1 space-y-0.5 text-2xs text-muted-foreground"
      data-testid="wiki-ask-dev-manifest"
    >
      {manifest.includedPages.length > 0 ? (
        <div>
          consulted:{" "}
          {manifest.includedPages.map((page) => page.title).join(", ")}
        </div>
      ) : null}
      {manifest.omittedPages.length > 0 ? (
        <div data-testid="wiki-ask-dev-omitted-pages">
          omitted: {manifest.omittedPages.map((page) => page.title).join(", ")}
        </div>
      ) : null}
      {manifest.includedSources.length > 0 ? (
        <div>
          sources:{" "}
          {manifest.includedSources
            .map((s) => `${s.path}:${s.startLine}-${s.endLine}`)
            .join(", ")}
        </div>
      ) : null}
      {manifest.omittedSources.length > 0 ? (
        <div data-testid="wiki-ask-dev-omitted-sources">
          omitted sources:{" "}
          {manifest.omittedSources
            .map((s) => `${s.path}:${s.startLine}-${s.endLine}`)
            .join(", ")}
        </div>
      ) : null}
    </div>
  );
}

/** The label a history row shows for a stored status. */
function historyStatusLabel(entry: PrivateAskHistoryEntry): string {
  switch (entry.status) {
    case "answered":
      return "answered";
    case "insufficient":
      return "not enough source";
    case "running":
      return "running";
    case "interrupted":
      return "interrupted";
    case "cancelled":
      return "cancelled";
    default:
      return entry.detail ?? "refused";
  }
}

const PHASE_LABEL: Record<string, string> = {
  retrieving: "Finding the relevant Wiki pages and source…",
  running: "The agent is answering…",
};

function PrivateAskDevComposer({
  coordinate,
  onOpenSource,
  projectId,
  scopeLabel,
  status,
}: {
  /** `<owner-hex>:<repo-d>`, or null when this pane is not showing one
   * repository. Without it there is nothing for an answer to be grounded in
   * and the composer says so rather than asking about an unnamed repository. */
  coordinate: string | null;
  onOpenSource?: (citation: PrivateAskCitation) => void;
  projectId: string;
  scopeLabel: string;
  status: PrivateAskDevStatus;
}) {
  const [state, dispatch] = React.useReducer(
    privateAskReducer,
    initialPrivateAskState,
  );
  /**
   * #367 — the editable task draft a validated answer opens. Produced by
   * `private_ask_draft` (the #366 seam), opened into the dispatch panel; the
   * panel owns the draft's lifecycle from there. `null` while closed.
   */
  const [draftInput, setDraftInput] =
    React.useState<PrivateAskDraftInput | null>(null);
  const [draftError, setDraftError] = React.useState<string | null>(null);
  /**
   * The agents this machine could address, and the one chosen. The picker
   * remembers per repository — a preference, not conversation state — keyed
   * by the coordinate it was chosen for.
   */
  const [agents, setAgents] = React.useState<PrivateAskAgent[]>([]);
  const [agentId, setAgentId] = React.useState<string>("");
  const [agentRecovery, setAgentRecovery] = React.useState<string | null>(null);

  const refreshAgents = React.useCallback(() => {
    invokeTauri<PrivateAskAgent[]>("private_ask_agents")
      .then((available) => {
        setAgents(available);
        setAgentId((chosen) => {
          if (available.some((agent) => agent.pubkey === chosen)) {
            return chosen;
          }
          const remembered = coordinate
            ? window.localStorage.getItem(rememberedAgentKey(coordinate))
            : null;
          if (
            remembered &&
            available.some(
              (agent) =>
                agent.pubkey === remembered && agent.status === "ready",
            )
          ) {
            return remembered;
          }
          return (
            available.find((agent) => agent.status === "ready")?.pubkey ??
            available[0]?.pubkey ??
            ""
          );
        });
      })
      .catch(() => {
        // A list that cannot be read is an empty list: the composer stays
        // usable and the Ask button stays disabled.
        setAgents([]);
        setAgentId("");
      });
  }, [coordinate]);

  React.useEffect(() => {
    refreshAgents();
  }, [refreshAgents]);

  /**
   * Read the owner-local record. It lives on this machine only — a private Ask
   * publishes nothing — so this is the only place a past attempt exists, and
   * it is what makes the list survive a restart. A record still `running`
   * re-attaches its attempt id so Stop names the owned run after navigation.
   */
  const refreshHistory = React.useCallback(
    () =>
      coordinate
        ? invokeTauri<PrivateAskHistoryEntry[]>("private_ask_history", {
            coordinate,
          })
            .then((history) => {
              dispatch({ type: "restored", history });
              const live = history.find((entry) => entry.status === "running");
              if (live) {
                dispatch({ type: "opened", entry: live });
              }
            })
            .catch(() => {
              // A record that cannot be read is an empty list, never a crash:
              // the composer must still be usable.
            })
        : Promise.resolve(),
    [coordinate],
  );

  React.useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  /**
   * #367 — an author returning through a kickoff's source backlink stashed
   * the private attempt id before navigating. Once the scoped history list
   * is here, reopen exactly that attempt — never another viewer's record
   * (the stash only exists when this viewer's own journal resolved it).
   */
  React.useEffect(() => {
    if (!coordinate) return;
    const focus = peekWikiAskFocus(coordinate);
    // Settle only once the scoped history has arrived — peeking before that
    // would drop the hint with the attempt still unrestored.
    if (!focus || state.history.length === 0) return;
    takeWikiAskFocus(coordinate);
    const entry = state.history.find(
      (candidate) => candidate.attemptId === focus.attemptId,
    );
    if (entry) {
      dispatch({ type: "opened", entry });
    }
  }, [coordinate, state.history]);

  const openDraft = React.useCallback(() => {
    if (!coordinate || !state.followUpOf) return;
    setDraftError(null);
    void invokeTauri<PrivateAskDraftInput | null>("private_ask_draft", {
      coordinate,
      attemptId: state.followUpOf,
    })
      .then((input) => {
        if (input) {
          setDraftInput(input);
        } else {
          setDraftError(
            "This answer is not on record as a draft source — ask again or reopen it from history.",
          );
        }
      })
      .catch((error: unknown) => {
        setDraftError(error instanceof Error ? error.message : String(error));
      });
  }, [coordinate, state.followUpOf]);

  /**
   * Progress is the attempt's own events, matched by the id this composer
   * minted — a renderer never infers a phase it was not told, and a stale
   * attempt's events drop at the reducer fence.
   */
  React.useEffect(() => {
    let unsubscribe: (() => void) | null = null;
    let disposed = false;
    subscribePrivateAskEvents({
      onProgress: (attemptId, phase) =>
        dispatch({ type: "phase", attemptId, phase }),
      onChunk: (attemptId, text) =>
        dispatch({ type: "chunk", attemptId, text }),
    })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          unsubscribe = unlisten;
        }
      })
      .catch(() => {
        // No progress feed — the settle result still arrives on the invoke.
      });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, []);

  const chooseAgent = React.useCallback(
    (next: string) => {
      setAgentId(next);
      if (coordinate) {
        window.localStorage.setItem(rememberedAgentKey(coordinate), next);
      }
    },
    [coordinate],
  );

  const ask = React.useCallback(() => {
    if (!coordinate || !canAskPrivately(state, status, agentId)) {
      return;
    }
    // Attempt and question ids are minted here, before the run starts: the
    // command only returns once the attempt is over, so an id taken from the
    // answer could never be cancelled — and the fence the stream events match
    // against is the id this surface chose, not one a result could forge.
    const attemptId = crypto.randomUUID();
    const questionId = state.followUpOf
      ? (state.questionId ?? crypto.randomUUID())
      : crypto.randomUUID();
    dispatch({
      type: "begin",
      attemptId,
      questionId,
      followUpOf: state.followUpOf,
    });
    invokeTauri<PrivateAskRunResult>("private_ask_run", {
      attemptId,
      questionId,
      agentId,
      coordinate,
      question: state.question,
      followUpOf: state.followUpOf,
    })
      .then((result) => {
        dispatch({ type: "settled", result });
        void refreshHistory();
      })
      .catch((error: unknown) => {
        dispatch({
          type: "rejected",
          attemptId,
          error: error instanceof Error ? error.message : String(error),
        });
        // A refusal is recorded on this machine too, so the list must catch up.
        void refreshHistory();
      });
  }, [agentId, coordinate, state, status, refreshHistory]);

  const stop = React.useCallback(() => {
    const attemptId = state.attemptId;
    if (!attemptId) {
      return;
    }
    dispatch({ type: "cancelled", attemptId });
    // A cancel the backend never heard of is a no-op there, so a failure here
    // is not worth surfacing to a developer who already sees an idle composer.
    void invokeTauri("private_ask_cancel", { attemptId })
      .catch(() => {})
      .then(() => void refreshHistory());
  }, [state.attemptId, refreshHistory]);

  const retry = React.useCallback(() => {
    // Retry keeps the question's thread but mints a NEW attempt — the old one
    // stays in history as its own record, and its late events are fenced out
    // by the new id.
    if (!coordinate || !canAskPrivately(state, status, agentId)) {
      return;
    }
    const attemptId = crypto.randomUUID();
    const questionId = state.questionId ?? crypto.randomUUID();
    dispatch({
      type: "begin",
      attemptId,
      questionId,
      followUpOf: state.followUpOf,
    });
    invokeTauri<PrivateAskRunResult>("private_ask_run", {
      attemptId,
      questionId,
      agentId,
      coordinate,
      question: state.question,
      followUpOf: state.followUpOf,
    })
      .then((result) => {
        dispatch({ type: "settled", result });
        void refreshHistory();
      })
      .catch((error: unknown) => {
        dispatch({
          type: "rejected",
          attemptId,
          error: error instanceof Error ? error.message : String(error),
        });
        void refreshHistory();
      });
  }, [agentId, coordinate, state, status, refreshHistory]);

  const openEntry = React.useCallback((entry: PrivateAskHistoryEntry) => {
    // Navigation detaches the UI, not the ownership contract: opening a
    // record shows its truthful stored state, and never retries a run whose
    // fate is unknown.
    dispatch({ type: "opened", entry });
  }, []);

  const forgetEntry = React.useCallback(
    (entry: PrivateAskHistoryEntry) => {
      if (!coordinate) {
        return;
      }
      void invokeTauri("private_ask_forget", {
        coordinate,
        attemptId: entry.attemptId,
      })
        .catch(() => {})
        .then(() => void refreshHistory());
    },
    [coordinate, refreshHistory],
  );

  const startAgent = React.useCallback(
    (agent: PrivateAskAgent) => {
      setAgentRecovery(agent.pubkey);
      void startManagedAgentRuntime(agent.pubkey, agent.relayUrl)
        .catch(() => {})
        .then(() => {
          setAgentRecovery(null);
          refreshAgents();
        });
    },
    [refreshAgents],
  );

  const inFlight = isAskInFlight(state);
  const outcomeVisible =
    state.phase === "answered" ||
    state.phase === "insufficient" ||
    state.phase === "refused" ||
    state.phase === "cancelled" ||
    state.phase === "interrupted";

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
          <select
            aria-label="Agent to ask"
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "h-8 px-2 text-2xs",
            )}
            data-testid="wiki-ask-dev-agent"
            disabled={agents.length === 0 || inFlight}
            onChange={(event) => chooseAgent(event.target.value)}
            value={agentId}
          >
            {agents.length === 0 ? (
              <option value="">No agent on this machine</option>
            ) : null}
            {agents.map((agent) => (
              <option
                disabled={agent.status === "busy"}
                key={agent.pubkey}
                value={agent.pubkey}
              >
                {agent.name}
                {agentAvailabilityLabel(agent.status)
                  ? ` (${agentAvailabilityLabel(agent.status)})`
                  : ""}
              </option>
            ))}
          </select>
          <textarea
            aria-label="Ask this agent privately"
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "min-w-0 flex-1 px-2 py-1 text-sm",
            )}
            data-testid="wiki-ask-dev-input"
            onChange={(event) =>
              dispatch({ type: "question", value: event.target.value })
            }
            onKeyDown={(event) => {
              // Enter asks; Shift+Enter is a newline; an in-flight IME
              // composition is never a submission.
              if (event.nativeEvent.isComposing) {
                return;
              }
              if (event.key !== "Enter" || event.shiftKey) {
                return;
              }
              event.preventDefault();
              ask();
            }}
            placeholder="Ask this agent privately"
            rows={1}
            value={state.question}
          />
          {inFlight ? (
            <Button
              data-testid="wiki-ask-dev-stop"
              onClick={stop}
              size="sm"
              type="button"
              variant="secondary"
            >
              Stop
            </Button>
          ) : (
            <>
              <Button
                data-testid="wiki-ask-dev-submit"
                disabled={
                  !coordinate || !canAskPrivately(state, status, agentId)
                }
                onClick={ask}
                size="sm"
                type="button"
              >
                Ask
              </Button>
              {canRetryAsk(state) ? (
                <Button
                  data-testid="wiki-ask-dev-retry"
                  onClick={retry}
                  size="sm"
                  type="button"
                  variant="secondary"
                >
                  Retry
                </Button>
              ) : null}
            </>
          )}
        </div>

        {state.followUpOf ? (
          <div
            className="mt-1 flex items-center gap-2 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-follow-up"
          >
            <span>Following up on the last answer.</span>
            <button
              className="text-primary"
              data-testid="wiki-ask-dev-follow-up-detach"
              onClick={() => dispatch({ type: "detachFollowUp" })}
              type="button"
            >
              Start a new question
            </button>
          </div>
        ) : null}

        {inFlight && PHASE_LABEL[state.phase] ? (
          <p
            aria-live="polite"
            className="mt-2 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-progress"
            role="status"
          >
            {PHASE_LABEL[state.phase]}
          </p>
        ) : null}

        {state.phase === "insufficient" ? (
          <div
            className="mt-2 rounded-md border border-border bg-muted/20 p-2 text-2xs"
            data-testid="wiki-ask-dev-insufficient"
            role="status"
          >
            Not enough source in this snapshot to answer that. What was
            consulted — and what was omitted — is listed below.
            <ManifestPanel manifest={state.manifest} />
          </div>
        ) : null}

        {state.phase === "interrupted" ? (
          <p
            aria-live="polite"
            className="mt-2 rounded-md border border-border bg-muted/20 p-2 text-2xs"
            data-testid="wiki-ask-dev-interrupted"
            role="status"
          >
            This attempt was interrupted before its outcome was recorded. It
            will not be retried on its own — use Retry to ask it again.
          </p>
        ) : null}

        {state.phase === "cancelled" ? (
          <p
            aria-live="polite"
            className="mt-2 rounded-md border border-border bg-muted/20 p-2 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-cancelled"
            role="status"
          >
            Stopped. The attempt's record is kept in your history.
          </p>
        ) : null}

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

        {!state.historyRecorded && outcomeVisible ? (
          <p
            aria-live="polite"
            className="mt-2 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-history-unrecorded"
            role="status"
          >
            This attempt could not be kept on this machine.
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
                    <CitationLink citation={citation} onOpen={onOpenSource} />
                  </li>
                ))}
              </ul>
            ) : null}
            {state.sourceRevision ? (
              <div
                className="mt-1 font-mono text-2xs text-muted-foreground"
                data-testid="wiki-ask-dev-source-revision"
              >
                source {state.sourceRevision}
              </div>
            ) : null}
            {state.phase === "answered" ? (
              <ManifestPanel manifest={state.manifest} />
            ) : null}
            {state.phase === "answered" && state.followUpOf ? (
              <div className="mt-2">
                {draftInput ? null : (
                  <Button
                    data-testid="wiki-ask-dev-draft"
                    onClick={openDraft}
                    size="sm"
                    type="button"
                    variant="secondary"
                  >
                    Create task draft
                  </Button>
                )}
                {draftError ? (
                  <p
                    aria-live="polite"
                    className="mt-1 text-2xs text-muted-foreground"
                    data-testid="wiki-ask-dev-draft-error"
                    role="status"
                  >
                    {draftError}
                  </p>
                ) : null}
                {draftInput ? (
                  <WikiTaskDispatchPanel
                    draft={draftInput}
                    onClose={() => setDraftInput(null)}
                    projectId={projectId}
                  />
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}

        {agents.some((agent) => agent.status === "offline") ? (
          <div
            className="mt-2 flex items-center gap-2 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-offline"
          >
            <span>
              {agents
                .filter((agent) => agent.status === "offline")
                .map((agent) => agent.name)
                .join(", ")}{" "}
              is offline.
            </span>
            {agents
              .filter((agent) => agent.status === "offline")
              .map((agent) => (
                <button
                  className="text-primary"
                  data-testid={`wiki-ask-dev-start-${agent.pubkey}`}
                  disabled={agentRecovery === agent.pubkey}
                  key={agent.pubkey}
                  onClick={() => startAgent(agent)}
                  type="button"
                >
                  {agentRecovery === agent.pubkey ? "Starting…" : "Start it"}
                </button>
              ))}
          </div>
        ) : null}

        {state.history.length > 0 ? (
          <ol
            className="mt-2 space-y-1 text-2xs text-muted-foreground"
            data-testid="wiki-ask-dev-history"
          >
            {state.history.map((entry) => (
              <li className="flex items-center gap-2" key={entry.attemptId}>
                <button
                  className="min-w-0 flex-1 truncate text-left"
                  data-testid={`wiki-ask-dev-history-entry-${entry.attemptId}`}
                  onClick={() => openEntry(entry)}
                  type="button"
                >
                  {entry.followUpOf ? "↳ " : ""}
                  {entry.question} — {historyStatusLabel(entry)}
                </button>
                <button
                  aria-label={`Forget this record`}
                  className="shrink-0 text-muted-foreground hover:text-foreground"
                  data-testid={`wiki-ask-dev-forget-${entry.attemptId}`}
                  onClick={() => forgetEntry(entry)}
                  type="button"
                >
                  ×
                </button>
              </li>
            ))}
          </ol>
        ) : null}
      </div>
    </div>
  );
}
