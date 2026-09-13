import * as React from "react";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import { normalizeRelayUrl } from "@/shared/lib/normalizeRelayUrl";
import {
  walkActiveAgentTurns,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import { subscribeControlResults } from "@/features/agents/controlResultDispatch";
import { awaitCancelTurnOutcome } from "@/features/agents/lib/cancelTurnOutcome";
import {
  awaitSteerTurnOutcome,
  type SteerTurnOutcome,
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
}: ThreadRunControlsProps) {
  const viewer = useIdentityQuery().data?.pubkey ?? "";
  const relay = normalizeRelayUrl(
    useCommunities().activeCommunity?.relayUrl ?? "",
  );
  const owned = useCurrentOwnedAgentPubkeys(viewer);
  const [, bump] = React.useReducer((value: number) => value + 1, 0);
  React.useEffect(() => subscribeActiveAgentTurns(bump), []);
  const steerInputId = React.useId();
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
      claimed: false,
      retire: () => {},
      // selectionKey includes every primitive identity field, not object reference.
    }),
    [selectionKey, viewer, relay, targetOwned],
  );
  const [feedback, setFeedback] = React.useState<{
    gate: typeof gate;
    text: string;
  } | null>(null);
  const [steerDraft, setSteerDraft] = React.useState("");
  React.useLayoutEffect(() => {
    gate.current = true;
    return () => {
      gate.current = false;
      gate.retire();
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
    if (!target || !eligible() || gate.claimed) return;
    gate.claimed = true;
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
          gate.retire = onTimeout;
          const timer = setTimeout(onTimeout, 10_000);
          return () => {
            clearTimeout(timer);
            gate.retire = () => {};
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
      if (outcome !== "sent" && outcome !== "unconfirmed") gate.claimed = false;
    } catch (error) {
      if (!gate.current) return;
      gate.claimed = !notAttempted;
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
    if (!publishSteer || !target || !prompt || !eligible() || gate.claimed)
      return;
    gate.claimed = true;
    setFeedback({
      gate,
      text: "Waiting for the selected run to acknowledge Steer…",
    });
    const requestId = crypto.randomUUID();
    let notAttempted = false;
    try {
      const outcome = await awaitSteerTurnOutcome({
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
        scheduleTimeout: (onTimeout) => {
          gate.retire = onTimeout;
          const timer = setTimeout(onTimeout, 10_000);
          return () => {
            clearTimeout(timer);
            gate.retire = () => {};
          };
        },
      });
      if (!gate.current) return;
      setFeedback({ gate, text: steerFeedback(outcome) });
      if (outcome === "appended") setSteerDraft("");
      // Only an unconfirmed send is replay-unsafe. The adapter's terminal
      // outcomes prove that this request can be retried safely if needed.
      gate.claimed = outcome === "unconfirmed";
    } catch (error) {
      if (!gate.current) return;
      gate.claimed = !notAttempted;
      setFeedback({
        gate,
        text:
          notAttempted && error instanceof Error
            ? error.message
            : "Steer is unconfirmed. Check the selected run before retrying.",
      });
    }
  };
  return (
    <div className="flex flex-col gap-2 p-2">
      {eligible() ? (
        <>
          <button
            type="button"
            disabled={gate.claimed}
            onClick={() => {
              void stop();
            }}
          >
            Stop selected run
          </button>
          {publishSteer ? (
            <div className="flex flex-col gap-1 text-sm">
              <label htmlFor={steerInputId}>Steer selected run</label>
              <textarea
                id={steerInputId}
                aria-label="Steer selected run"
                value={steerDraft}
                disabled={gate.claimed}
                maxLength={16 * 1024}
                onChange={(event) => setSteerDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !event.shiftKey) {
                    event.preventDefault();
                    void steer();
                  }
                }}
                placeholder="Send guidance to this run"
                rows={2}
              />
              <button
                type="button"
                disabled={gate.claimed || !steerDraft.trim()}
                onClick={() => {
                  void steer();
                }}
              >
                Steer selected run
              </button>
            </div>
          ) : null}
        </>
      ) : null}
      {feedback?.gate === gate ? <p role="status">{feedback.text}</p> : null}
    </div>
  );
}

function steerFeedback(outcome: SteerTurnOutcome): string {
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
      return "The selected runtime rejected Steer. You can retry.";
    case "unconfirmed":
      return "Steer is unconfirmed. Check the selected run before retrying.";
  }
}
