import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import { normalizeRelayUrl } from "@/shared/lib/normalizeRelayUrl";
import { normalizePubkey } from "@/shared/lib/pubkey";

/** View identity only; resource ownership remains with the governor. */
export type ThreadToolPaneScope = {
  relayUrl: string;
  viewerPubkey: string;
  channelId: string;
  rootEventId: string;
};

/** Reject roots that cannot identify an existing Buzz conversation. */
export function threadToolPaneScopeKey(
  scope: ThreadToolPaneScope,
): string | null {
  const relayUrl = normalizeRelayUrl(scope.relayUrl);
  const viewerPubkey = normalizePubkey(scope.viewerPubkey);
  const channelId = scope.channelId.toLowerCase();
  if (
    !/^wss?:\/\//.test(relayUrl) ||
    !/^[0-9a-f]{64}$/.test(viewerPubkey) ||
    !deriveAgentConversationIdOrNull(channelId, scope.rootEventId)
  )
    return null;
  return JSON.stringify([relayUrl, viewerPubkey, channelId, scope.rootEventId]);
}
