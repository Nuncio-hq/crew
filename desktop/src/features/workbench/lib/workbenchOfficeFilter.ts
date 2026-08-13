import { parseEvidenceKind } from "@/features/messages/lib/evidenceTag";
import { KIND_AGENT_RECEIPT } from "@/shared/constants/kinds";
import type { WorkbenchTranscriptRow } from "./workbenchTranscript";

/**
 * Office view is a presentation filter over the same transcript, not a
 * second app. Keep kickoff, receipts, evidence, and questions. Hide tool
 * calls, ROLE-CHECK thoughts, observer chrome, and sleep/wake lines.
 */
export function isOfficeVisibleRow(
  row: WorkbenchTranscriptRow,
  threadHeadId: string,
): boolean {
  switch (row.type) {
    case "catch-up":
      return true;
    case "user-input":
      return true;
    case "message":
      return isOfficeVisibleMessage(row, threadHeadId);
    case "observer":
    case "sleep-wake":
      return false;
    default: {
      const _exhaustive: never = row;
      return _exhaustive;
    }
  }
}

function isOfficeVisibleMessage(
  row: Extract<WorkbenchTranscriptRow, { type: "message" }>,
  threadHeadId: string,
): boolean {
  if (row.message.id === threadHeadId) return true;
  if (row.message.kind === KIND_AGENT_RECEIPT) return true;
  return parseEvidenceKind(row.message.tags) !== null;
}

export function isRoleCheckObserverItem(item: {
  renderClass?: string;
  text?: string;
  title?: string;
}): boolean {
  if (item.renderClass === "thought") {
    const blob = `${item.title ?? ""} ${item.text ?? ""}`;
    return /ROLE-CHECK/i.test(blob);
  }
  return /ROLE-CHECK/i.test(`${item.title ?? ""} ${item.text ?? ""}`);
}

export const OFFICE_EXPLANATION =
  "Office view shows what the channel audience sees: the kickoff, receipts, evidence, and questions. Tool calls, ROLE-CHECK lines, and session controls stay in the workbench.";
