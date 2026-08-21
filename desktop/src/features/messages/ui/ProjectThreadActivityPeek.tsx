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
  formatProjectThreadPeekText,
  getProjectThreadPeekHeadline,
  mergeProjectThreadPeekEvents,
  previewProjectThreadPeekText,
  resolveProjectThreadPeekMode,
  type ProjectThreadPeekFeedItem,
  type ProjectThreadPeekToolStatus,
} from "../lib/projectThreadMissionControl";
import type { ProjectThreadWorkspaceModel } from "./useProjectThreadWorkspaceModel";

/**
 * Live / history activity strip above the thread composer.
 *
 * Visual language mirrors Declared Plans (#190): muted uppercase chrome,
 * soft bordered cards, scannable rows — not a raw ACP dump. Tool output and
 * long thoughts stay collapsed until the founder opens them.
 */
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
      className="relative z-10 shrink-0 border-t border-border/50 bg-background/95"
      data-mode={mode}
      data-testid="project-thread-activity-peek"
    >
      {expanded ? (
        <div
          className="max-h-56 space-y-1.5 overflow-y-auto px-3 pt-2.5 pb-1"
          data-testid="project-thread-peek-feed"
          ref={feedRef}
          role="log"
        >
          <h2 className="truncate px-0.5 text-2xs font-semibold tracking-wide text-muted-foreground uppercase">
            {mode === "history" ? "Recent activity" : "Live activity"}
          </h2>
          <div className="flex min-w-0 flex-col gap-1.5">
            {feed.map((item) =>
              item.kind === "thinking" ? (
                <PeekThinkingRow item={item} key={item.id} />
              ) : (
                <PeekToolRow item={item} key={item.id} />
              ),
            )}
          </div>
        </div>
      ) : null}
      <button
        aria-expanded={expanded}
        className="flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/35"
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
          <Clock3
            aria-hidden
            className="h-3.5 w-3.5 shrink-0 text-muted-foreground"
          />
        )}
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          <span className="font-medium text-foreground">
            {model.activeName}
          </span>
          <span> · {headline}</span>
        </span>
        <span
          className={cn(
            "shrink-0 rounded-md px-1.5 py-0.5 text-2xs font-semibold tracking-wide uppercase",
            mode === "live"
              ? "bg-blue-500/10 text-blue-600 dark:text-blue-400"
              : "bg-muted/60 text-muted-foreground",
          )}
        >
          {mode === "history" ? "History" : "Live"}
        </span>
        <ChevronDown
          aria-hidden
          className={cn(
            "h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform motion-reduce:transition-none",
            expanded && "rotate-180",
          )}
        />
      </button>
    </section>
  );
}

function PeekThinkingRow({
  item,
}: {
  item: Extract<ProjectThreadPeekFeedItem, { kind: "thinking" }>;
}) {
  const { preview, truncated } = previewProjectThreadPeekText(item.text);
  const full = formatProjectThreadPeekText(item.text);

  if (!truncated) {
    return (
      <p
        className="rounded-lg border border-border/60 bg-card/50 px-2.5 py-1.5 text-xs leading-5 text-muted-foreground"
        data-testid="project-thread-peek-thinking"
      >
        <span className="mr-1.5 text-2xs font-semibold tracking-wide text-muted-foreground/80 uppercase">
          Thinking
        </span>
        {preview}
      </p>
    );
  }

  return (
    <details
      className="group rounded-lg border border-border/60 bg-card/50 open:bg-card/70"
      data-testid="project-thread-peek-thinking"
    >
      <summary className="flex cursor-pointer list-none items-start gap-2 px-2.5 py-1.5 text-xs leading-5 text-muted-foreground [&::-webkit-details-marker]:hidden">
        <span className="mt-0.5 shrink-0 text-2xs font-semibold tracking-wide text-muted-foreground/80 uppercase">
          Thinking
        </span>
        <span className="min-w-0 flex-1 truncate group-open:hidden">
          {preview}
        </span>
        <span className="mt-0.5 hidden min-w-0 flex-1 text-2xs text-muted-foreground/70 group-open:inline">
          Hide
        </span>
        <ChevronDown
          aria-hidden
          className="mt-0.5 h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-180 motion-reduce:transition-none"
        />
      </summary>
      <p className="max-h-28 overflow-y-auto border-t border-border/50 px-2.5 py-1.5 text-xs leading-5 whitespace-pre-wrap text-muted-foreground">
        {full}
      </p>
    </details>
  );
}

function PeekToolRow({
  item,
}: {
  item: Extract<ProjectThreadPeekFeedItem, { kind: "tool" }>;
}) {
  const resultPreview = item.result
    ? previewProjectThreadPeekText(item.result)
    : null;
  const fullResult = item.result
    ? formatProjectThreadPeekText(item.result)
    : null;
  const showResultDisclosure = Boolean(fullResult && resultPreview?.truncated);

  return (
    <div
      className={cn(
        "rounded-lg border px-2.5 py-1.5",
        item.failed
          ? "border-destructive/35 bg-destructive/5"
          : "border-border/60 bg-card/50",
      )}
      data-testid="project-thread-peek-tool"
    >
      <div className="flex min-w-0 items-start gap-2">
        <PeekToolStatusDot status={item.status} />
        <p
          className={cn(
            "min-w-0 flex-1 truncate text-xs font-medium leading-5 text-foreground",
            item.failed && "text-destructive",
          )}
        >
          {item.headline}
        </p>
        <span
          className={cn(
            "shrink-0 text-2xs font-medium tracking-wide uppercase",
            item.status === "failed" && "text-destructive",
            item.status === "running" && "text-blue-600 dark:text-blue-400",
            item.status === "done" && "text-muted-foreground",
          )}
        >
          {peekToolStatusLabel(item.status)}
        </span>
      </div>
      {fullResult && !showResultDisclosure ? (
        <p
          className={cn(
            "mt-1 pl-4 text-2xs leading-4 text-muted-foreground",
            item.failed && "text-destructive/80",
          )}
          data-testid="project-thread-peek-tool-result"
        >
          {resultPreview?.preview}
        </p>
      ) : null}
      {fullResult && showResultDisclosure ? (
        <details className="group/result mt-1 pl-4">
          <summary
            className={cn(
              "flex cursor-pointer list-none items-center gap-1 text-2xs text-muted-foreground hover:text-foreground [&::-webkit-details-marker]:hidden",
              item.failed && "text-destructive/80 hover:text-destructive",
            )}
          >
            <ChevronDown
              aria-hidden
              className="h-3 w-3 shrink-0 transition-transform group-open/result:rotate-180 motion-reduce:transition-none"
            />
            <span className="min-w-0 truncate group-open/result:hidden">
              {resultPreview?.preview || "Show output"}
            </span>
            <span className="hidden group-open/result:inline">Hide output</span>
          </summary>
          <pre
            className={cn(
              "mt-1 max-h-24 overflow-auto rounded-md border border-border/50 bg-muted/30 px-2 py-1.5 font-mono text-2xs leading-4 whitespace-pre-wrap text-muted-foreground",
              item.failed && "border-destructive/25 text-destructive/90",
            )}
            data-testid="project-thread-peek-tool-result"
          >
            {fullResult}
          </pre>
        </details>
      ) : null}
    </div>
  );
}

function PeekToolStatusDot({
  status,
}: {
  status: ProjectThreadPeekToolStatus;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full",
        status === "running" &&
          "animate-pulse bg-blue-500 motion-reduce:animate-none",
        status === "done" && "bg-emerald-500/80",
        status === "failed" && "bg-destructive",
      )}
      data-status={status}
    />
  );
}

function peekToolStatusLabel(status: ProjectThreadPeekToolStatus): string {
  switch (status) {
    case "running":
      return "Running";
    case "done":
      return "Done";
    case "failed":
      return "Failed";
    default: {
      const exhaustive: never = status;
      return exhaustive;
    }
  }
}
