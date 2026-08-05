import * as React from "react";

import type { useAddChannelMembersMutation } from "@/features/channels/hooks";
import type { UseMentionsResult } from "@/features/messages/lib/useMentions";
import type { ManagedAgent } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  MENTION_REFERENCE_TAG,
  mergeOutgoingTagsWithReferenceMentions,
  type PendingNonMemberMentionSend,
  uniqueNormalizedPubkeys,
} from "./useMentionSendFlow.helpers";

type UseMentionSendInviteHandlersOptions = {
  addMembersMutation: ReturnType<typeof useAddChannelMembersMutation>;
  completeSend: (
    draft: PendingNonMemberMentionSend,
    mentionPubkeys: string[],
    outgoingTags?: string[][],
  ) => Promise<void>;
  getManagedAgentsByPubkey: () => Promise<Map<string, ManagedAgent>>;
  mentions: Pick<UseMentionsResult, "isAgentPubkey">;
  pendingNonMemberSend: PendingNonMemberMentionSend | null;
  setNonMemberPromptError: React.Dispatch<React.SetStateAction<string | null>>;
};

export function useMentionSendInviteHandlers({
  addMembersMutation,
  completeSend,
  getManagedAgentsByPubkey,
  mentions,
  pendingNonMemberSend,
  setNonMemberPromptError,
}: UseMentionSendInviteHandlersOptions) {
  const handleSendWithoutInviting = React.useCallback(() => {
    if (!pendingNonMemberSend) return;
    const nonMemberPubkeys = new Set(
      pendingNonMemberSend.nonMemberPubkeys.map((pubkey) =>
        normalizePubkey(pubkey),
      ),
    );
    const mentionPubkeys = pendingNonMemberSend.mentionPubkeys.filter(
      (pubkey) => !nonMemberPubkeys.has(normalizePubkey(pubkey)),
    );
    void completeSend(
      pendingNonMemberSend,
      mentionPubkeys,
      mergeOutgoingTagsWithReferenceMentions(
        pendingNonMemberSend.outgoingTags,
        nonMemberPubkeys,
      ),
    );
  }, [completeSend, pendingNonMemberSend]);

  const handleInviteNonMembers = React.useCallback(() => {
    if (!pendingNonMemberSend) return;
    const invitedPubkeys = new Set(
      pendingNonMemberSend.nonMemberPubkeys.map(normalizePubkey),
    );
    const mentionPubkeys = uniqueNormalizedPubkeys([
      ...pendingNonMemberSend.mentionPubkeys,
      ...pendingNonMemberSend.nonMemberPubkeys,
    ]);
    const outgoingTags = (pendingNonMemberSend.outgoingTags ?? []).filter(
      (tag) =>
        tag[0] !== MENTION_REFERENCE_TAG ||
        !invitedPubkeys.has(normalizePubkey(tag[1] ?? "")),
    );
    setNonMemberPromptError(null);
    void (async () => {
      const managedAgentsByPubkey = await getManagedAgentsByPubkey();
      const peoplePubkeys: string[] = [];
      const relayAgentPubkeys: string[] = [];
      for (const pubkey of uniqueNormalizedPubkeys(
        pendingNonMemberSend.nonMemberPubkeys,
      )) {
        if (managedAgentsByPubkey.has(pubkey)) continue;
        if (mentions.isAgentPubkey(pubkey)) relayAgentPubkeys.push(pubkey);
        else peoplePubkeys.push(pubkey);
      }
      const errors: string[] = [];
      if (peoplePubkeys.length > 0) {
        const result = await addMembersMutation.mutateAsync({
          channelId: pendingNonMemberSend.capturedChannelId ?? undefined,
          pubkeys: peoplePubkeys,
          role: "member",
        });
        errors.push(...result.errors.map((error) => error.error));
      }
      if (relayAgentPubkeys.length > 0) {
        const result = await addMembersMutation.mutateAsync({
          channelId: pendingNonMemberSend.capturedChannelId ?? undefined,
          pubkeys: relayAgentPubkeys,
          role: "bot",
        });
        errors.push(...result.errors.map((error) => error.error));
      }
      if (errors.length > 0) {
        setNonMemberPromptError(errors.join("; "));
        return;
      }
      await completeSend(
        { ...pendingNonMemberSend, mentionPubkeys, outgoingTags },
        mentionPubkeys,
        outgoingTags,
      );
    })().catch((error) => {
      setNonMemberPromptError(
        error instanceof Error ? error.message : "Could not invite members.",
      );
    });
  }, [
    addMembersMutation,
    completeSend,
    getManagedAgentsByPubkey,
    mentions.isAgentPubkey,
    pendingNonMemberSend,
    setNonMemberPromptError,
  ]);

  return { handleInviteNonMembers, handleSendWithoutInviting };
}
