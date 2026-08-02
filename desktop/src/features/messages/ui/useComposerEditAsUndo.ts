import * as React from "react";

import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import { useEditAsUndoUiState } from "@/features/agents/useEditAsUndoState";
import {
  diffAddedMentionPubkeys,
  diffRemovedMentionPubkeys,
} from "@/features/messages/lib/threading";
import { normalizePubkey } from "@/shared/lib/pubkey";

type EditTarget = { id: string; body: string } | null | undefined;

/**
 * Edit-as-undo affordance + mention-diff helpers for the composer edit path.
 *
 * Keeps MessageComposer under the file-size ratchet by owning the pre-dispatch
 * undo derivation and the added/removed mention sets published on kind:40003.
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

  const computeEditMentionDiffs = React.useCallback(
    (originalBody: string, editedBody: string, selfPubkey: string) => {
      const originalMentionPubkeys = extractMentionPubkeys(originalBody);
      const editedMentionPubkeys = extractMentionPubkeys(editedBody);
      return {
        addedMentionPubkeys: diffAddedMentionPubkeys(
          originalMentionPubkeys,
          editedMentionPubkeys,
          selfPubkey,
        ),
        removedMentionPubkeys: diffRemovedMentionPubkeys(
          originalMentionPubkeys,
          editedMentionPubkeys,
          selfPubkey,
        ),
      };
    },
    [extractMentionPubkeys],
  );

  return {
    editAsUndoState,
    computeEditMentionDiffs,
  };
}
