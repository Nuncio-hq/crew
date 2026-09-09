import { invokeTauri } from "./tauri";
import type {
  OwnerOperationScope,
  ScopedOwnerOperation,
} from "./ownerOperations";

export type CrewConfigDraft = {
  definitions: { label: string; definition: string }[];
  assignments: Record<string, string>;
  contact: string | null;
  renames: Record<string, string>;
  preserved_assignments: Record<string, string>;
  remove_routing: string[];
  remove_capabilities: string[];
};

export type CrewSaveProgress = {
  operation_id: string;
  outcome:
    | "not_committed"
    | "commit_uncertain"
    | "canvas_committed_announcement_pending"
    | "applied"
    | "superseded";
  canvas_event_id: string;
  current_event_id: string | null;
  automatic_retry_at: number | null;
  manual_retry_required: boolean;
};

export type CrewSaveResult =
  | {
      result: "conflict" | "review_required" | "unchanged";
      current_event_id: string | null;
    }
  | { result: "recovery_pending"; operation_id: string }
  | { result: "saved"; progress: CrewSaveProgress };

export function saveChannelCrewConfig(
  expected: OwnerOperationScope,
  channelId: string,
  expectedCanvasEventId: string | null,
  draft: CrewConfigDraft,
): Promise<ScopedOwnerOperation<CrewSaveResult>> {
  return invokeTauri("save_channel_crew_config", {
    expected,
    channelId,
    expectedCanvasEventId,
    draft,
  });
}

export function retryChannelCrewConfig(
  expected: OwnerOperationScope,
  operationId: string,
): Promise<ScopedOwnerOperation<CrewSaveProgress>> {
  return invokeTauri("retry_channel_crew_config", { expected, operationId });
}

export function listChannelCrewOperations(
  expected: OwnerOperationScope,
  channelId: string,
): Promise<ScopedOwnerOperation<CrewSaveProgress[]>> {
  return invokeTauri("list_channel_crew_operations", { expected, channelId });
}

export function getChannelCrewOperation(
  expected: OwnerOperationScope,
  operationId: string,
): Promise<ScopedOwnerOperation<CrewSaveProgress>> {
  return invokeTauri("get_channel_crew_operation", { expected, operationId });
}
