import * as React from "react";

import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import { useEditAsUndoUiState } from "@/features/agents/useEditAsUndoState";
import { normalizePubkey } from "@/shared/lib/pubkey";

import type { MessageComposerEditTarget } from "./MessageComposer.types";

type EditTarget = MessageComposerEditTarget | null | undefined;

/**
 * Edit-as-undo affordance for the composer edit path.
 *
 * Keeps MessageComposer under the file-size ratchet by owning the
 * pre-dispatch undo derivation. Added/removed mention diffs for kind:40003
 * live in `submitMessageEdit` — the single call site that publishes edits.
 */
export function useComposerEditAsUndo({
  editTarget,
}: {
  editTarget: EditTarget;
}) {
  const knownAgentPubkeys = useKnownAgentPubkeys();
  const editMentionsAgent = React.useMemo(() => {
    if (!editTarget) {
      return false;
    }
    // Historical event identities are authoritative even before draft hydration.
    // Resolving old text against today's picker can throw or select a namesake.
    return [
      ...(editTarget.mentionRefs ?? []).map((ref) => ref.pubkey),
      ...(editTarget.unresolvedMentionPubkeys ?? []),
    ].some((pubkey) => knownAgentPubkeys.has(normalizePubkey(pubkey)));
  }, [editTarget, knownAgentPubkeys]);

  const editAsUndoState = useEditAsUndoUiState({
    mentionsAgent: editMentionsAgent,
    eventId: editTarget?.id,
  });

  return {
    editAsUndoState,
  };
}
