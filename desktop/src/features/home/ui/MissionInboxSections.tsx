import {
  AlertTriangle,
  ChevronDown,
  Circle,
  ExternalLink,
  Hammer,
  Zap,
  PackageCheck,
} from "lucide-react";
import * as React from "react";

import type {
  MissionInboxRow,
  MissionInboxSections,
} from "@/features/home/lib/missionInbox";
import { snoozeAgentAttention } from "@/features/agents/agentAttentionSnoozeStore";
import { useOrgRosterQuery } from "@/features/org/hooks/useOrgRosterQuery";
import { isOfficer, managerOf } from "@/features/org/lib/orgRoster";
import { useMyRelayMembershipQuery } from "@/features/community-members/hooks";

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
    return <Zap className="h-3.5 w-3.5 text-attention" />;
  if (state === "readyToReview")
    return <PackageCheck className="h-3.5 w-3.5 text-success" />;
  if (state === "possiblyStalled")
    return <AlertTriangle className="h-3.5 w-3.5 text-attention" />;
  if (state === "failed" || state === "lostContact")
    return <AlertTriangle className="h-3.5 w-3.5 text-destructive" />;
  if (state === "telemetryUnavailable")
    return <Circle className="h-3 w-3 text-muted-foreground" />;
  return <Circle className="h-3 w-3 fill-sky-500 text-sky-500" />;
}

function stateLabel(state: MissionInboxRow["state"]): string {
  switch (state) {
    case "needsYou":
      return "Needs you";
    case "failed":
      return "Failed";
    case "lostContact":
      return "Lost contact";
    case "telemetryUnavailable":
      return "Telemetry unavailable";
    case "possiblyStalled":
      return "Possibly stalled";
    case "readyToReview":
      return "Ready to review";
    default:
      return "Working";
  }
}

function MissionRow({
  row,
  onSelect,
  onOpenChannel,
  onOpenWorkbench,
  selected,
}: {
  row: MissionInboxRow;
  onSelect: (row: MissionInboxRow) => void;
  onOpenChannel: (row: MissionInboxRow) => void;
  onOpenWorkbench?: (row: MissionInboxRow) => void;
  selected: boolean;
}) {
  const isException = row.state !== "working" && row.state !== "readyToReview";
  return (
    <div
      className="group/mission relative border-b border-border/35 px-3 py-2"
      data-state={row.state}
    >
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
            {stateLabel(row.state)} · {row.agentPubkey || "Agent"} ·{" "}
            {row.phaseOrHeadline}
          </span>
          {row.escalationHop ? (
            <span className="block truncate text-2xs text-muted-foreground">
              {row.escalationHop}
            </span>
          ) : null}
        </span>
        <span className="shrink-0 text-2xs text-muted-foreground">
          {ageLabel(row.age)}
        </span>
      </button>
      {isException ? (
        <div className="ml-5 mt-1 flex items-center gap-1.5">
          {row.state === "possiblyStalled" ? (
            <button
              className="rounded-md border border-attention/40 px-2 py-0.5 text-2xs font-medium text-attention transition-colors hover:bg-attention/10"
              data-testid={`mission-inbox-wait-${row.conversationId}`}
              onClick={() => snoozeAgentAttention(row.conversationId)}
              type="button"
            >
              Wait 10m
            </button>
          ) : null}
          <button
            className="rounded-md border border-border px-2 py-0.5 text-2xs font-medium text-foreground transition-colors hover:bg-accent"
            data-testid={`mission-inbox-inspect-${row.conversationId}`}
            onClick={() => onSelect(row)}
            type="button"
          >
            {row.state === "needsYou" ? "Respond" : "Inspect"}
          </button>
        </div>
      ) : null}
      <button
        aria-label="Open workbench"
        className="absolute right-8 top-1/2 -translate-y-1/2 rounded-md bg-background/90 p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        data-testid={`mission-inbox-workbench-${row.conversationId}`}
        onClick={(event) => {
          event.stopPropagation();
          onOpenWorkbench?.(row);
        }}
        type="button"
      >
        <Hammer className="h-3.5 w-3.5" />
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
  onOpenWorkbench,
  onSelect,
  selectedConversationId,
}: {
  sections: MissionInboxSections;
  onOpenChannel: (row: MissionInboxRow) => void;
  onOpenWorkbench?: (row: MissionInboxRow) => void;
  onSelect: (row: MissionInboxRow) => void;
  selectedConversationId?: string | null;
}) {
  const [workingOpen, setWorkingOpen] = React.useState(false);
  const [showAllLevels, setShowAllLevels] = React.useState(false);
  const roster = useOrgRosterQuery().data;
  const isFounder = useMyRelayMembershipQuery().data?.role === "owner";
  const needsYouRows = React.useMemo(() => {
    if (!isFounder || !roster || showAllLevels) {
      return sections.needsYou;
    }
    return sections.needsYou.filter((row) => {
      const manager = managerOf(roster, row.agentPubkey);
      return (
        manager === roster.founderPubkey || isOfficer(roster, row.agentPubkey)
      );
    });
  }, [isFounder, roster, sections.needsYou, showAllLevels]);
  const groups = [
    { key: "needsYou", label: "Needs attention", rows: needsYouRows },
    {
      key: "readyToReview",
      label: "Ready to review",
      rows: sections.readyToReview,
    },
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
          {group.key === "needsYou" && isFounder ? (
            <button
              className="px-3 pb-1 text-2xs text-primary"
              data-testid="mission-inbox-show-all-levels"
              onClick={() => setShowAllLevels((value) => !value)}
              type="button"
            >
              {showAllLevels ? "Officer level" : "Show all levels"}
            </button>
          ) : null}
          {group.rows.length > 0 ? (
            group.rows.map((row) => (
              <MissionRow
                key={row.conversationId}
                onOpenChannel={onOpenChannel}
                onOpenWorkbench={onOpenWorkbench}
                onSelect={onSelect}
                row={row}
                selected={selectedConversationId === row.conversationId}
              />
            ))
          ) : (
            <p className="px-3 pb-2 text-xs text-muted-foreground">
              {group.key === "needsYou"
                ? "Nothing needs attention — safe to close"
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
                onOpenWorkbench={onOpenWorkbench}
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
