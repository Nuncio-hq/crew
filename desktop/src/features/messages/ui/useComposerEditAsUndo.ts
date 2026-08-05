import * as React from "react";

import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import { useEditAsUndoUiState } from "@/features/agents/useEditAsUndoState";
import { normalizePubkey } from "@/shared/lib/pubkey";

type EditTarget = { id: string; body: string } | null | undefined;

/**
 * Edit-as-undo affordance for the composer edit path.
 *
 * Keeps MessageComposer under the file-size ratchet by owning the
 * pre-dispatch undo derivation. Added/removed mention diffs for kind:40003
 * live in `submitMessageEdit` — the single call site that publishes edits.
 */
export function useComposerEditAsUndo({
  editTarget,
  extractMentionPubkeys,
}: {
  editTarget: EditTarget;
  extractMentionPubkeys: (body: string) => string[];
}) {
  const knownAgentPubkeys = useKnownAgentPubkeys();
  const editMentionsAgent = React.useMemo(() => {
    if (!editTarget) {
      return false;
    }
    return extractMentionPubkeys(editTarget.body).some((pubkey) =>
      knownAgentPubkeys.has(normalizePubkey(pubkey)),
    );
  }, [editTarget, extractMentionPubkeys, knownAgentPubkeys]);

  const editAsUndoState = useEditAsUndoUiState({
    mentionsAgent: editMentionsAgent,
    eventId: editTarget?.id,
  });

  return {
    editAsUndoState,
  };
}
