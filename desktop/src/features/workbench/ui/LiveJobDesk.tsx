import * as React from "react";
import { useReducedMotion } from "motion/react";
import { AGENT_ACTIVITY_CHROME } from "@/features/agents/ui/agentActivityChrome";
import { useComposerAgentStop } from "@/features/channels/ui/useComposerAgentStop";
import { useThreadComposerFocus } from "@/features/messages/lib/threadComposerFocusRegistry";
import { THREAD_PANEL_MESSAGE_GUTTER_CLASS } from "@/features/messages/lib/messageThreadPanelLayout";
import { ThreadSelectedRunControls } from "@/features/tool-pane/ThreadSelectedRunControls";
import { openThreadToolPane } from "@/features/tool-pane/toolPaneStore";
import {
  threadRunKey,
  useThreadRunControlPublisher,
} from "@/features/tool-pane/useThreadRunControlPublisher";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import type { LiveThreadRun } from "../lib/liveJobDesk";
import { useLiveJobDesk } from "../hooks/useLiveJobDesk";

/**
 * The one thread-level Steer/Stop surface, directly under the thread head.
 *
 * It offers controls bound to an exact run when the thread has exactly one —
 * the same run identities Activity lists. With several, choosing for the
 * operator would be guessing, so it points at Activity. With none known (or a
 * run this viewer does not own), it keeps the honest agent-scoped controls:
 * Steer focuses the composer, Stop stops the agent.
 */
export function LiveJobDesk({
  channelId,
  threadRootId,
}: {
  channelId: string;
  threadRootId: string;
}) {
  const desk = useLiveJobDesk({ channelId, threadRootId });
  const only = desk.runs.length === 1 ? desk.runs[0] : null;
  // The strip vanishes the instant the run ends, taking its Stop confirmation
  // with it. Leave the run's own terminal outcome in its place for a bounded
  // window so the operator sees the ledger's cancelled outcome — an operator
  // Stop, or a harness teardown that cancelled the run out from under it.
  if (!desk.show)
    return desk.outcome?.outcome === "cancelled" ? (
      <StoppedRunTrace
        endedAt={desk.outcome.endedAt}
        name={desk.nameFor(desk.outcome.agentPubkey)}
      />
    ) : null;
  if (only && desk.conversationId) {
    return (
      <SingleRunDesk
        agentName={desk.nameFor(only.agentPubkey)}
        channelId={channelId}
        conversationId={desk.conversationId}
        fallbackName={desk.targetName}
        fallbackPubkey={desk.targetPubkey}
        run={only}
        threadRootId={threadRootId}
      />
    );
  }
  return (
    <DeskRow>
      {desk.runs.length > 1 ? (
        <>
          <DeskText>
            {AGENT_ACTIVITY_CHROME.runsWorking(desk.runs.length)}
          </DeskText>
          <Button
            data-testid="live-job-desk-activity"
            onClick={() => openThreadToolPane("activity")}
            size="sm"
            type="button"
            variant="outline"
          >
            {AGENT_ACTIVITY_CHROME.openActivity}
          </Button>
        </>
      ) : (
        <AgentScopedControls
          channelId={channelId}
          conversationId={desk.conversationId}
          name={desk.targetName}
          pubkey={desk.targetPubkey}
          threadRootId={threadRootId}
        />
      )}
    </DeskRow>
  );
}

/** Shared strip frame: one row when it fits, text on its own row when narrow. */
function DeskRow({ children }: { children: React.ReactNode }) {
  return (
    <div
      className={cn(
        THREAD_PANEL_MESSAGE_GUTTER_CLASS,
        "flex flex-wrap items-center gap-2 pb-2",
      )}
      data-testid="live-job-desk"
    >
      {children}
    </div>
  );
}

/**
 * The strip's sentence. `min-w-40` keeps it readable: once the column cannot
 * hold that plus the buttons, the whole row wraps and the buttons drop to a
 * second line instead of squeezing the text into one word per line.
 */
function DeskText({ children }: { children: React.ReactNode }) {
  return (
    <p className="min-w-40 flex-1 truncate text-xs text-muted-foreground">
      {children}
    </p>
  );
}

/**
 * No exact run identity is known for this thread (or the live run is not this
 * viewer's to control), so both controls name the agent they act on.
 */
function AgentScopedControls({
  channelId,
  conversationId,
  name,
  pubkey,
  threadRootId,
}: {
  channelId: string;
  conversationId: string | null;
  name: string;
  pubkey: string | null;
  threadRootId: string;
}) {
  const { stopAgent } = useComposerAgentStop({ channelId, conversationId });
  // The composer registers its own focus function under this thread's root
  // id (see `threadComposerFocusRegistry`); no entry means no composer is
  // mounted for this thread, so Steer is disabled rather than a silent
  // no-op against a stale or absent DOM selector.
  const focusComposer = useThreadComposerFocus(threadRootId);
  return (
    <>
      <DeskText>{AGENT_ACTIVITY_CHROME.agentWorkingHint(name)}</DeskText>
      <Button
        data-testid="live-job-desk-steer"
        disabled={!focusComposer}
        onClick={() => focusComposer?.()}
        size="sm"
        type="button"
        variant="ghost"
      >
        {AGENT_ACTIVITY_CHROME.steerAgent(name)}
      </Button>
      <Button
        data-testid="live-job-desk-stop"
        disabled={!pubkey}
        onClick={() => {
          if (pubkey) void stopAgent(pubkey, name);
        }}
        size="sm"
        type="button"
        variant="outline"
      >
        {AGENT_ACTIVITY_CHROME.stopAgent(name)}
      </Button>
    </>
  );
}

/**
 * The unambiguous case: one live run, bound by its exact identity.
 *
 * Owner capture and publication live in the shared publisher hook, so this
 * surface cannot publish under a scope Activity would not have accepted. A
 * viewer who does not own the run keeps the agent-scoped controls.
 */
function SingleRunDesk({
  agentName,
  channelId,
  conversationId,
  fallbackName,
  fallbackPubkey,
  run,
  threadRootId,
}: {
  agentName: string;
  channelId: string;
  conversationId: string;
  fallbackName: string;
  fallbackPubkey: string | null;
  run: LiveThreadRun;
  threadRootId: string;
}) {
  const publisher = useThreadRunControlPublisher({
    channelId,
    rootEventId: threadRootId,
    conversationId,
    agentPubkey: run.agentPubkey,
  });
  const { token } = publisher;
  const selection = publisher.buildSelection(run.sessionId, run.turnId);
  const { publishStop, publishSteer } = publisher.publishersFor(
    token ? { run: selection, token } : null,
  );
  if (!publisher.owned || !token) {
    return (
      <DeskRow>
        <AgentScopedControls
          channelId={channelId}
          conversationId={conversationId}
          name={fallbackName}
          pubkey={fallbackPubkey}
          threadRootId={threadRootId}
        />
      </DeskRow>
    );
  }
  return (
    <DeskRow>
      <DeskText>
        <span data-testid="live-job-desk-run-agent">
          {agentName} {AGENT_ACTIVITY_CHROME.isWorking}
        </span>
        {run.liveness ? (
          <span className="ml-2 rounded-full border border-border/60 px-1.5 py-0.5 text-2xs uppercase tracking-wide">
            {run.liveness}
          </span>
        ) : null}
      </DeskText>
      {/* Keyed by run identity: a draft or feedback for one run can never be
          shown for, or sent to, the run that replaced it. */}
      <ThreadSelectedRunControls
        compact
        key={threadRunKey(run)}
        publishSteer={publishSteer}
        publishStop={publishStop}
        selection={selection}
      />
    </DeskRow>
  );
}

/** How long the neutral stopped trace stays where the strip was. */
const STOPPED_TRACE_MS = 8_000;
const STOPPED_TRACE_FADE_MS = 500;

/**
 * The run's own cancellation outcome, rendered in the strip's place.
 *
 * It reads the same conversation outcome ledger the thread status chip reads,
 * so no new event and no separate UI state can disagree with it. The window is
 * measured from the recorded end time, not from mount, so a remount inside the
 * window shows the remainder rather than restarting the clock.
 */
function StoppedRunTrace({ endedAt, name }: { endedAt: number; name: string }) {
  const reduceMotion = useReducedMotion() ?? false;
  const holdMs = reduceMotion
    ? STOPPED_TRACE_MS
    : STOPPED_TRACE_MS + STOPPED_TRACE_FADE_MS;
  const [phase, setPhase] = React.useState<"visible" | "fading" | "gone">(() =>
    Date.now() - endedAt >= holdMs ? "gone" : "visible",
  );
  React.useEffect(() => {
    // A clock that moved backwards must not extend the window past its budget.
    const elapsed = Math.max(0, Date.now() - endedAt);
    if (elapsed >= holdMs) {
      setPhase("gone");
      return;
    }
    setPhase(
      elapsed >= STOPPED_TRACE_MS && !reduceMotion ? "fading" : "visible",
    );
    const timers = [
      setTimeout(() => setPhase("gone"), holdMs - elapsed),
      ...(reduceMotion || elapsed >= STOPPED_TRACE_MS
        ? []
        : [setTimeout(() => setPhase("fading"), STOPPED_TRACE_MS - elapsed)]),
    ];
    return () => {
      for (const timer of timers) clearTimeout(timer);
    };
  }, [endedAt, holdMs, reduceMotion]);
  if (phase === "gone") return null;
  return (
    <DeskRow>
      <DeskText>
        <span
          className={cn(
            reduceMotion ? undefined : "transition-opacity duration-500",
            phase === "fading" ? "opacity-0" : "opacity-100",
          )}
          data-testid="live-job-desk-stopped"
          role="status"
        >
          {name} · {AGENT_ACTIVITY_CHROME.runStopped}
        </span>
      </DeskText>
    </DeskRow>
  );
}
