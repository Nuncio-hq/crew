import * as React from "react";
import { AGENT_ACTIVITY_CHROME } from "@/features/agents/ui/agentActivityChrome";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import { normalizeRelayUrl } from "@/shared/lib/normalizeRelayUrl";
import {
  walkActiveAgentTurns,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import { subscribeControlResults } from "@/features/agents/controlResultDispatch";
import {
  awaitCancelTurnOutcome,
  STOP_UI_BUDGET_MS,
} from "@/features/agents/lib/cancelTurnOutcome";
import {
  awaitSteerTurnOutcome,
  STEER_PROMPT_MAX_BYTES,
  STEER_UI_BUDGET_MS,
  steerPromptByteLength,
  type SteerTurnOutcome,
  type SteerTurnOutcomeResult,
} from "@/features/agents/lib/steerTurnOutcome";

/** Exact run selected in the thread Activity view. */
export type ThreadRunSelection = Readonly<{
  relayUrl: string;
  viewerPubkey: string;
  channelId: string;
  rootEventId: string;
  conversationId: string;
  agentPubkey: string;
  sessionId: string;
  turnId: string;
}>;

/** Native scoped publication is supplied by the thread's control adapter. */
export type ThreadRunControlsProps = {
  selection: ThreadRunSelection | null;
  publishStop: (
    selection: ThreadRunSelection,
    requestId: string,
  ) => Promise<{
    status: "accepted" | "unknown" | "not_attempted";
    message?: string;
  }>;
  /** Compact row for the inline thread strip: Steer is a disclosure. */
  compact?: boolean;
  publishSteer?: (
    selection: ThreadRunSelection,
    requestId: string,
    prompt: string,
  ) => Promise<{
    status: "accepted" | "unknown" | "not_attempted";
    message?: string;
  }>;
};

function isLive(selection: ThreadRunSelection | null): boolean {
  if (!selection || Object.values(selection).some((value) => !value.trim()))
    return false;
  let matches = 0;
  walkActiveAgentTurns((agent, turn) => {
    if (
      agent === selection.agentPubkey &&
      turn.channelId === selection.channelId &&
      turn.conversationId === selection.conversationId &&
      turn.sessionId === selection.sessionId &&
      turn.turnId === selection.turnId
    )
      matches += 1;
  });
  return matches === 1;
}

/** Controls remain attached to one explicit selection; history never targets a successor. */
export function ThreadSelectedRunControls({
  selection,
  publishStop,
  publishSteer,
  compact = false,
}: ThreadRunControlsProps) {
  const viewer = useIdentityQuery().data?.pubkey ?? "";
  const relay = normalizeRelayUrl(
    useCommunities().activeCommunity?.relayUrl ?? "",
  );
  const owned = useCurrentOwnedAgentPubkeys(viewer);
  const [, bump] = React.useReducer((value: number) => value + 1, 0);
  React.useEffect(() => subscribeActiveAgentTurns(bump), []);
  const steerInputId = React.useId();
  // The two steer inputs coexist on screen, so their accessible names must
  // differ: assistive tech would otherwise present one target twice.
  const steerInputLabel = compact
    ? AGENT_ACTIVITY_CHROME.steerThisRunLabel
    : AGENT_ACTIVITY_CHROME.steerSelectedRunLabel;
  const steerBudgetId = React.useId();
  // The inline strip keeps the steer input behind a disclosure so the row
  // stays one line until the operator asks for it.
  const [steerOpen, setSteerOpen] = React.useState(!compact);
  const steerToggleRef = React.useRef<HTMLButtonElement>(null);
  const closeSteerInput = () => {
    if (!compact) return;
    setSteerOpen(false);
    steerToggleRef.current?.focus();
  };
  const selectionKey = JSON.stringify(selection);
  const targetOwned = !!selection && owned.has(selection.agentPubkey);
  const gate = React.useMemo(
    () => ({
      selection: Object.freeze(
        JSON.parse(selectionKey),
      ) as ThreadRunSelection | null,
      viewer,
      relay,
      targetOwned,
      current: true,
      // Steer and Stop latch independently: an unconfirmed Steer must not
      // disable Stop, which is the operator's only way back.
      steerClaimed: false,
      stopClaimed: false,
      // A set, not a slot: Stop and Steer can both be waiting, and a single
      // slot would let the second overwrite the first's teardown.
      disposers: new Set<() => void>(),
      // selectionKey includes every primitive identity field, not object reference.
    }),
    [selectionKey, viewer, relay, targetOwned],
  );
  const [feedback, setFeedback] = React.useState<{
    gate: typeof gate;
    text: string;
  } | null>(null);
  const [steerDraft, setSteerDraft] = React.useState("");
  // The native control counts UTF-8 bytes, so the composer must too: a
  // character cap would admit a prompt the adapter then refuses.
  const promptBytes = steerPromptByteLength(steerDraft);
  const promptBytesLeft = STEER_PROMPT_MAX_BYTES - promptBytes;
  const promptOverBudget = promptBytesLeft < 0;
  React.useLayoutEffect(() => {
    gate.current = true;
    return () => {
      gate.current = false;
      for (const dispose of [...gate.disposers]) dispose();
      gate.disposers.clear();
    };
  }, [gate]);
  const target = gate.selection;
  const eligible = () =>
    gate.current &&
    target !== null &&
    target.viewerPubkey === viewer &&
    normalizeRelayUrl(target.relayUrl) === relay &&
    gate.targetOwned &&
    isLive(target);
  const stop = async () => {
    if (!target || !eligible() || gate.stopClaimed) return;
    gate.stopClaimed = true;
    setFeedback({
      gate,
      text: "Waiting for the selected run to acknowledge Stop…",
    });
    const requestId = crypto.randomUUID();
    let notAttempted = false;
    try {
      const outcome = await awaitCancelTurnOutcome({
        requestId,
        channelId: target.channelId,
        conversationId: target.conversationId,
        subscribe: (listener) =>
          subscribeControlResults(target.agentPubkey, (frame) => {
            if (frame.turnId === target.turnId) listener(frame);
          }),
        sendCancel: async () => {
          const result = await publishStop(target, requestId);
          if (result.status === "not_attempted") {
            notAttempted = true;
            throw new Error(
              result.message ?? "Stop was not sent. You can retry.",
            );
          }
          // Unknown delivery still waits for the correlated harness result.
        },
        scheduleTimeout: (onTimeout) => {
          const timer = setTimeout(onTimeout, STOP_UI_BUDGET_MS);
          const dispose = () => {
            clearTimeout(timer);
            gate.disposers.delete(dispose);
            onTimeout();
          };
          gate.disposers.add(dispose);
          return () => {
            clearTimeout(timer);
            gate.disposers.delete(dispose);
          };
        },
      });
      if (!gate.current) return;
      const text =
        outcome === "sent"
          ? "Stop signal accepted for the selected run."
          : outcome === "cancelled_queued"
            ? "Queued work was cancelled."
            : outcome === "unconfirmed"
              ? "Stop is unconfirmed. Check the selected run before retrying."
              : "The selected run is no longer available to stop.";
      setFeedback({ gate, text });
      // An unconfirmed send may already have taken effect; do not automatically replay it.
      if (outcome !== "sent" && outcome !== "unconfirmed")
        gate.stopClaimed = false;
    } catch (error) {
      if (!gate.current) return;
      gate.stopClaimed = !notAttempted;
      setFeedback({
        gate,
        text:
          notAttempted && error instanceof Error
            ? error.message
            : "Stop is unconfirmed. Check the selected run before retrying.",
      });
    }
  };
  const steer = async () => {
    const prompt = steerDraft.trim();
    if (
      !publishSteer ||
      !target ||
      !prompt ||
      promptOverBudget ||
      !eligible() ||
      gate.steerClaimed
    )
      return;
    gate.steerClaimed = true;
    setFeedback({
      gate,
      text: "Waiting for the selected run to acknowledge Steer…",
    });
    const requestId = crypto.randomUUID();
    let notAttempted = false;
    try {
      const applyOutcome = (settled: SteerTurnOutcomeResult) => {
        setFeedback({
          gate,
          text: steerFeedback(settled.outcome, settled.error),
        });
        if (settled.outcome === "appended") setSteerDraft("");
        // Only an unconfirmed send is replay-unsafe. The adapter's terminal
        // outcomes prove that this request can be retried safely if needed.
        gate.steerClaimed = settled.outcome === "unconfirmed";
      };
      let pendingDispose: () => void = () => {};
      const pending = awaitSteerTurnOutcome({
        requestId,
        channelId: target.channelId,
        conversationId: target.conversationId,
        sessionId: target.sessionId,
        turnId: target.turnId,
        subscribe: (listener) =>
          subscribeControlResults(target.agentPubkey, (frame) => {
            if (frame.turnId === target.turnId) listener(frame);
          }),
        sendSteer: async () => {
          const result = await publishSteer(target, requestId, prompt);
          if (result.status === "not_attempted") {
            notAttempted = true;
            throw new Error(
              result.message ?? "Steer was not sent. You can retry.",
            );
          }
        },
        // The UI budget must outlast the adapter's own request deadline;
        // otherwise its `expired` answer always arrives too late to be seen.
        scheduleTimeout: (onTimeout) => {
          const timer = setTimeout(onTimeout, STEER_UI_BUDGET_MS);
          return () => clearTimeout(timer);
        },
        // A terminal outcome that lands after the UI budget downgrades an
        // unconfirmed request to a retryable one and releases the claim.
        onLateOutcome: (late) => {
          gate.disposers.delete(pendingDispose);
          if (!gate.current) return;
          applyOutcome(late);
        },
      });
      pendingDispose = pending.dispose;
      gate.disposers.add(pendingDispose);
      const result = await pending.result;
      if (!gate.current) return;
      applyOutcome(result);
    } catch (error) {
      if (!gate.current) return;
      gate.steerClaimed = !notAttempted;
      setFeedback({
        gate,
        text:
          notAttempted && error instanceof Error
            ? error.message
            : "Steer is unconfirmed. Check the selected run before retrying.",
      });
    }
  };
  const controlsEnabled = eligible();
  return (
    <div
      className={
        compact
          ? "flex flex-col gap-1.5"
          : controlsEnabled
            ? "flex flex-col gap-2 rounded-lg border border-border/60 bg-muted/20 p-3"
            : "flex flex-col gap-2 p-2"
      }
    >
      {controlsEnabled ? (
        <>
          <div className={compact ? "flex items-center gap-1.5" : "contents"}>
            {compact && publishSteer ? (
              <button
                type="button"
                ref={steerToggleRef}
                aria-expanded={steerOpen}
                aria-controls={steerInputId}
                className="inline-flex h-7 items-center justify-center rounded-md border border-border bg-background px-2.5 text-xs font-medium text-foreground shadow-xs transition-colors hover:bg-muted/70 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background"
                onClick={() => setSteerOpen((open) => !open)}
              >
                {AGENT_ACTIVITY_CHROME.steerRun}
              </button>
            ) : null}
            <button
              type="button"
              className={
                compact
                  ? "inline-flex h-7 items-center justify-center rounded-md border border-border bg-background px-2.5 text-xs font-medium text-foreground shadow-xs transition-colors hover:bg-muted/70 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50"
                  : "inline-flex h-8 items-center justify-center rounded-md border border-border bg-background px-3 text-xs font-medium text-foreground shadow-xs transition-colors hover:bg-muted/70 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50"
              }
              disabled={gate.stopClaimed}
              onClick={() => {
                void stop();
              }}
            >
              {compact
                ? AGENT_ACTIVITY_CHROME.stopRun
                : AGENT_ACTIVITY_CHROME.stopSelectedRun}
            </button>
          </div>
          {publishSteer && steerOpen ? (
            <div className="flex flex-col gap-1.5 text-sm">
              <label
                className={
                  compact ? "sr-only" : "text-xs font-medium text-foreground"
                }
                htmlFor={steerInputId}
              >
                {steerInputLabel}
              </label>
              <textarea
                id={steerInputId}
                aria-label={steerInputLabel}
                className="min-h-16 w-full resize-y rounded-md border border-input/60 bg-background px-3 py-2 text-sm text-foreground shadow-xs outline-hidden transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50"
                value={steerDraft}
                disabled={gate.steerClaimed}
                aria-describedby={steerBudgetId}
                aria-invalid={promptOverBudget}
                onChange={(event) => setSteerDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape" && compact) {
                    event.preventDefault();
                    closeSteerInput();
                    return;
                  }
                  if (event.key === "Enter" && !event.shiftKey) {
                    event.preventDefault();
                    void steer();
                  }
                }}
                placeholder="Send guidance to this run"
                rows={2}
              />
              <p
                className={
                  promptOverBudget
                    ? "text-2xs text-destructive"
                    : "text-2xs text-muted-foreground"
                }
                id={steerBudgetId}
              >
                {promptOverBudget
                  ? `${-promptBytesLeft} bytes over the ${STEER_PROMPT_MAX_BYTES} byte limit`
                  : `${promptBytesLeft} bytes left`}
              </p>
              <button
                type="button"
                className="inline-flex h-8 items-center justify-center self-start rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground shadow transition-colors hover:bg-primary/90 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50"
                disabled={
                  gate.steerClaimed || !steerDraft.trim() || promptOverBudget
                }
                onClick={() => {
                  void steer();
                }}
              >
                {compact
                  ? AGENT_ACTIVITY_CHROME.sendSteer
                  : AGENT_ACTIVITY_CHROME.steerSelectedRunLabel}
              </button>
            </div>
          ) : null}
        </>
      ) : null}
      {feedback?.gate === gate ? (
        <p
          className="rounded-md border border-border/60 bg-background/60 px-2.5 py-2 text-xs text-foreground"
          role="status"
        >
          {feedback.text}
        </p>
      ) : null}
    </div>
  );
}

function steerFeedback(outcome: SteerTurnOutcome, reason?: string): string {
  switch (outcome) {
    case "appended":
      return "Steer appended to the selected run.";
    case "stale_target":
      return "The selected run has ended or changed.";
    case "busy":
      return "The selected run is already processing a steer. You can retry.";
    case "expired":
      return "Steer expired before the selected run reached a round boundary. You can retry.";
    case "rejected":
      return reason
        ? `The selected runtime rejected Steer (${reason}). You can retry.`
        : "The selected runtime rejected Steer. You can retry.";
    case "unconfirmed":
      return "Steer is unconfirmed. Check the selected run before retrying.";
  }
}
