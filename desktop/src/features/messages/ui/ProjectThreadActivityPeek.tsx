import { ChevronDown, Clock3 } from "lucide-react";
import * as React from "react";

import { buildTranscriptState } from "@/features/agents/ui/agentSessionTranscript";
import {
  useArchivedChannelEvents,
  useLoadArchivedObserverEvents,
  useObserverEvents,
} from "@/features/agents/ui/useObserverEvents";

import { cn } from "@/shared/lib/cn";
import {
  createProjectThreadPeekFeedSelector,
  getProjectThreadPeekHeadline,
  mergeProjectThreadPeekEvents,
  resolveProjectThreadPeekMode,
} from "../lib/projectThreadMissionControl";
import type { ProjectThreadWorkspaceModel } from "./useProjectThreadWorkspaceModel";

export function ProjectThreadActivityPeek({
  channelId,
  model,
}: {
  channelId: string | null;
  model: ProjectThreadWorkspaceModel | null;
}) {
  const [expanded, setExpanded] = React.useState(false);
  const agentPubkey = model?.activePubkey ?? null;
  const conversationId = model?.conversationId ?? null;
  const active =
    model?.steps.some((step) => step.status === "working") ?? false;

  const liveSnapshot = useObserverEvents(Boolean(agentPubkey), agentPubkey);
  const archivedEvents = useArchivedChannelEvents(agentPubkey, channelId);
  useLoadArchivedObserverEvents(Boolean(agentPubkey && channelId), channelId);

  const conversationEvents = React.useMemo(
    () =>
      mergeProjectThreadPeekEvents(
        liveSnapshot.events,
        archivedEvents,
        conversationId,
      ),
    [archivedEvents, conversationId, liveSnapshot.events],
  );
  const transcript = React.useMemo(
    () => buildTranscriptState(conversationEvents).items,
    [conversationEvents],
  );
  const feedSelectorRef = React.useRef(createProjectThreadPeekFeedSelector());
  const feed = feedSelectorRef.current(transcript);
  const mode = resolveProjectThreadPeekMode(active, feed.length);
  const headline = getProjectThreadPeekHeadline(transcript) ?? "Working";
  const feedRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    if (!expanded || feed.length === 0) return;
    const feedElement = feedRef.current;
    if (feedElement) feedElement.scrollTop = feedElement.scrollHeight;
  }, [expanded, feed]);

  if (!model || !agentPubkey || mode === "hidden") return null;

  return (
    <section
      className="relative z-10 shrink-0 border-t border-border/50 bg-background"
      data-mode={mode}
      data-testid="project-thread-activity-peek"
    >
      {expanded ? (
        <div
          className="max-h-56 space-y-2 overflow-y-auto px-3 py-2"
          data-testid="project-thread-peek-feed"
          ref={feedRef}
          role="log"
        >
          {feed.map((item) =>
            item.kind === "thinking" ? (
              <p
                className="text-xs italic text-muted-foreground"
                data-testid="project-thread-peek-thinking"
                key={item.id}
              >
                {item.text}
              </p>
            ) : (
              <div
                className="rounded-md border border-border/60 bg-muted/20 px-2 py-1.5 font-mono text-xs"
                data-testid="project-thread-peek-tool"
                key={item.id}
              >
                <p className={cn(item.failed && "text-destructive")}>
                  {item.headline}
                </p>
                {item.result ? (
                  <p
                    className={cn(
                      "mt-1 whitespace-pre-wrap text-2xs text-muted-foreground",
                      item.failed && "text-destructive/80",
                    )}
                    data-testid="project-thread-peek-tool-result"
                  >
                    {item.result}
                  </p>
                ) : null}
              </div>
            ),
          )}
        </div>
      ) : null}
      <button
        aria-expanded={expanded}
        className="flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left text-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground"
        data-testid="project-thread-peek-toggle"
        onClick={() => setExpanded((current) => !current)}
        type="button"
      >
        {mode === "live" ? (
          <span
            aria-hidden
            className="h-2 w-2 shrink-0 animate-pulse rounded-full bg-blue-500 motion-reduce:animate-none"
          />
        ) : (
          <Clock3 aria-hidden className="h-3.5 w-3.5 shrink-0" />
        )}
        <span className="min-w-0 flex-1 truncate">
          <span className="font-medium text-foreground">
            {model.activeName}
          </span>
          <span> · {headline}</span>
        </span>
        <span className="shrink-0 text-2xs font-medium uppercase tracking-wide">
          {mode === "history" ? "History" : "Live"}
        </span>
        <ChevronDown
          aria-hidden
          className={cn(
            "h-3.5 w-3.5 shrink-0 transition-transform motion-reduce:transition-none",
            expanded && "rotate-180",
          )}
        />
      </button>
    </section>
  );
}
