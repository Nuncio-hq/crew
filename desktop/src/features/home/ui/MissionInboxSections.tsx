import {
  ChevronDown,
  Circle,
  ExternalLink,
  Zap,
  PackageCheck,
} from "lucide-react";
import * as React from "react";

import type {
  MissionInboxRow,
  MissionInboxSections,
} from "@/features/home/lib/missionInbox";

function ageLabel(ageMs: number) {
  const minutes = Math.max(0, Math.floor(ageMs / 60_000));
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function SectionIcon({ state }: { state: MissionInboxRow["state"] }) {
  if (state === "needsYou")
    return <Zap className="h-3.5 w-3.5 text-amber-500" />;
  if (state === "readyToReview")
    return <PackageCheck className="h-3.5 w-3.5 text-emerald-500" />;
  return <Circle className="h-3 w-3 fill-sky-500 text-sky-500" />;
}

function MissionRow({
  row,
  onSelect,
  onOpenChannel,
  selected,
}: {
  row: MissionInboxRow;
  onSelect: (row: MissionInboxRow) => void;
  onOpenChannel: (row: MissionInboxRow) => void;
  selected: boolean;
}) {
  return (
    <div className="group/mission relative border-b border-border/35 px-3 py-2">
      <button
        aria-label={`Open ${row.threadTitle}`}
        className={`flex w-full min-w-0 items-start gap-2 rounded-md text-left focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring ${selected ? "bg-muted/50" : ""}`}
        data-testid={`mission-inbox-row-${row.conversationId}`}
        onClick={() => onSelect(row)}
        type="button"
      >
        <span className="mt-0.5 shrink-0">
          <SectionIcon state={row.state} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-semibold text-foreground">
            {row.threadTitle}
          </span>
          <span className="block truncate text-xs text-muted-foreground">
            {row.agentPubkey || "Agent"} · {row.phaseOrHeadline}
          </span>
        </span>
        <span className="shrink-0 text-2xs text-muted-foreground">
          {ageLabel(row.age)}
        </span>
      </button>
      <button
        aria-label="Open in channel"
        className="absolute right-2 top-1/2 hidden -translate-y-1/2 rounded-md bg-background/90 p-1 text-muted-foreground hover:text-foreground group-hover/mission:block focus-visible:block"
        data-testid={`mission-inbox-channel-${row.conversationId}`}
        onClick={(event) => {
          event.stopPropagation();
          onOpenChannel(row);
        }}
        type="button"
      >
        <ExternalLink className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

export function MissionInboxSectionsView({
  sections,
  onOpenChannel,
  onSelect,
  selectedConversationId,
}: {
  sections: MissionInboxSections;
  onOpenChannel: (row: MissionInboxRow) => void;
  onSelect: (row: MissionInboxRow) => void;
  selectedConversationId?: string | null;
}) {
  const [workingOpen, setWorkingOpen] = React.useState(false);
  const groups = [
    { key: "needsYou", label: "Needs you", rows: sections.needsYou },
    { key: "readyToReview", label: "Ready to review", rows: sections.readyToReview },
  ] as const;
  return (
    <div
      className="border-b border-border/60 bg-background/40"
      data-testid="mission-inbox-sections"
    >
      {groups.map((group) => (
        <section
          key={group.key}
          data-testid={`mission-inbox-section-${group.key}`}
        >
          <div className="flex items-center justify-between px-3 pb-1 pt-3">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              {group.label}
            </h2>
            <span className="text-2xs text-muted-foreground">
              {group.rows.length}
            </span>
          </div>
          {group.rows.length > 0 ? (
            group.rows.map((row) => (
              <MissionRow
                key={row.conversationId}
                onOpenChannel={onOpenChannel}
                onSelect={onSelect}
                row={row}
                selected={selectedConversationId === row.conversationId}
              />
            ))
          ) : (
            <p className="px-3 pb-2 text-xs text-muted-foreground">
              {group.key === "needsYou"
                ? "Nothing needs you — safe to close"
                : "Nothing ready to review"}
            </p>
          )}
        </section>
      ))}
      <section data-testid="mission-inbox-section-working">
        <button
          aria-expanded={workingOpen}
          className="flex w-full items-center justify-between px-3 pb-2 pt-3 text-left"
          onClick={() => setWorkingOpen((open) => !open)}
          type="button"
        >
          <span className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            {workingOpen ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronDown className="h-3 w-3 -rotate-90" />
            )}{" "}
            In flight
          </span>
          <span className="text-2xs text-muted-foreground">
            {sections.working.length}
          </span>
        </button>
        {workingOpen
          ? sections.working.map((row) => (
              <MissionRow
                key={row.conversationId}
                onOpenChannel={onOpenChannel}
                onSelect={onSelect}
                row={row}
                selected={selectedConversationId === row.conversationId}
              />
            ))
          : null}
      </section>
    </div>
  );
}
