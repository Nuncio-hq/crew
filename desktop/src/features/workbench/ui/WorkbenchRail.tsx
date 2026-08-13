import { cn } from "@/shared/lib/cn";
import type { WorkbenchLens } from "../lib/workbenchRoutes";
import type {
  WorkbenchAgentGroup,
  WorkbenchChannelGroup,
  WorkbenchThreadRow,
} from "../lib/workbenchThreadIndex";
import { WorkbenchRailRow } from "./WorkbenchRailRow";

export function WorkbenchRail({
  agentGroups,
  channelGroups,
  lens,
  onLensChange,
  onSelect,
  selectedThreadRootId,
}: {
  agentGroups: readonly WorkbenchAgentGroup[];
  channelGroups: readonly WorkbenchChannelGroup[];
  lens: WorkbenchLens;
  onLensChange: (lens: WorkbenchLens) => void;
  onSelect: (row: WorkbenchThreadRow) => void;
  selectedThreadRootId?: string | null;
}) {
  return (
    <aside
      className="flex w-[16.5rem] shrink-0 flex-col border-r border-border/60 bg-background"
      data-testid="workbench-rail"
    >
      <div className="flex gap-1 border-b border-border/60 p-2">
        <LensButton
          active={lens === "thread"}
          label="By thread"
          onClick={() => onLensChange("thread")}
          testId="workbench-lens-thread"
        />
        <LensButton
          active={lens === "agent"}
          label="By agent"
          onClick={() => onLensChange("agent")}
          testId="workbench-lens-agent"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1 py-2">
        {lens === "thread"
          ? channelGroups.map((group) => (
              <section
                className="mb-3"
                data-testid={`workbench-channel-group-${group.channelId}`}
                key={group.channelId}
              >
                <h2 className="px-2 pb-1 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
                  # {group.channelName}
                </h2>
                {group.threads.map((row) => (
                  <WorkbenchRailRow
                    key={row.conversationId}
                    onSelect={onSelect}
                    row={row}
                    selected={row.threadRootId === selectedThreadRootId}
                  />
                ))}
              </section>
            ))
          : agentGroups.map((group) => (
              <section
                className="mb-3"
                data-testid={`workbench-agent-group-${group.pubkey || "unassigned"}`}
                key={group.pubkey || "unassigned"}
              >
                <h2 className="px-2 pb-1 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
                  {group.name}
                </h2>
                {group.threads.map((row) => (
                  <WorkbenchRailRow
                    key={`${group.pubkey}:${row.conversationId}`}
                    onSelect={onSelect}
                    row={row}
                    selected={row.threadRootId === selectedThreadRootId}
                  />
                ))}
              </section>
            ))}
      </div>
    </aside>
  );
}

function LensButton({
  active,
  label,
  onClick,
  testId,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
  testId: string;
}) {
  return (
    <button
      className={cn(
        "flex-1 rounded-md px-2 py-1 text-xs font-medium",
        active
          ? "bg-muted text-foreground"
          : "text-muted-foreground hover:bg-muted/50",
      )}
      data-testid={testId}
      onClick={onClick}
      type="button"
    >
      {label}
    </button>
  );
}
