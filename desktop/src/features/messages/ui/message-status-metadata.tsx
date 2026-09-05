import type { EditAsUndoUiState } from "@/features/agents/dispatchedEventIds";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

/** The same status metadata accompanies either a full row or its continuation. */
export function renderMessageStatusMetadata({
  pending,
  edited,
  editAsUndoState,
}: {
  pending?: boolean;
  edited?: boolean;
  editAsUndoState: EditAsUndoUiState | null;
}) {
  const editAsUndoInline =
    editAsUndoState === "too-late"
      ? "Agent already read the original"
      : editAsUndoState === "withdrawn"
        ? "Request withdrawn — agent never ran"
        : null;

  return pending || edited || editAsUndoInline ? (
    <>
      {pending ? (
        <p
          className="font-normal text-muted-foreground/70"
          data-testid="message-send-status"
        >
          Sending…
        </p>
      ) : null}
      {edited ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <p className="text-muted-foreground/70">(edited)</p>
          </TooltipTrigger>
          <TooltipContent>This message has been edited</TooltipContent>
        </Tooltip>
      ) : null}
      {editAsUndoInline ? (
        <p
          className="font-normal text-muted-foreground/70"
          data-testid="message-edit-as-undo-status"
        >
          {editAsUndoInline}
        </p>
      ) : null}
    </>
  ) : null;
}
