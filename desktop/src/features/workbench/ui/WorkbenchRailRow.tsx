import { WorkThreadRow } from "@/features/work-tree/ui/WorkThreadRow";
import type {
  WorkThreadRowModel,
  WorkThreadStatus,
} from "@/features/work-tree/lib/workTreeTypes";
import type {
  WorkbenchAgentStatus,
  WorkbenchThreadRow,
} from "../lib/workbenchThreadIndex";

function workThreadStatusFromWorkbench(
  status: WorkbenchAgentStatus,
): WorkThreadStatus {
  switch (status) {
    case "needs-you":
      return "needs-you";
    case "working":
      return "working";
    case "sleeping":
    case "ready":
    case "idle":
    case "failed":
      return "sleeping";
    default: {
      const _exhaustive: never = status;
      return _exhaustive;
    }
  }
}

export function workbenchRowToWorkThread(
  row: WorkbenchThreadRow,
): WorkThreadRowModel {
  return {
    branch: null,
    channelId: row.channelId,
    channelName: row.channelName,
    ciGlyph: null,
    conversationId: row.conversationId,
    hasWorkspaceBinding: row.prNumber != null,
    lastActivityAt: 0,
    prNumber: row.prNumber,
    prTone: row.prNumber ? "open" : null,
    status: workThreadStatusFromWorkbench(row.status),
    threadRootId: row.threadRootId,
    title: row.title,
    unread: row.unread,
  };
}

export function WorkbenchRailRow({
  onSelect,
  row,
  selected,
}: {
  onSelect: (row: WorkbenchThreadRow) => void;
  row: WorkbenchThreadRow;
  selected: boolean;
}) {
  return (
    <WorkThreadRow
      now={0}
      onSelect={() => onSelect(row)}
      row={workbenchRowToWorkThread(row)}
      selected={selected}
      testId={`workbench-rail-row-${row.threadRootId}`}
    />
  );
}
