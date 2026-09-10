import * as React from "react";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import {
  bindThreadToolPaneView,
  captureThreadToolPaneRemoval,
} from "./toolPaneStore";

/** Use the active community and viewer, never a channel-owned identity fallback. */
export function useThreadToolPaneScope(
  channelId?: string | null,
  rootEventId?: string | null,
) {
  const { activeCommunity } = useCommunities();
  const identity = useIdentityQuery();
  const relayUrl = activeCommunity?.relayUrl ?? "";
  const viewerPubkey = identity.data?.pubkey ?? "";
  return React.useMemo(
    () => ({
      relayUrl,
      viewerPubkey,
      channelId: channelId ?? "",
      rootEventId: rootEventId ?? "",
    }),
    [relayUrl, viewerPubkey, channelId, rootEventId],
  );
}

/** Thread navigation owns presentation only; cleanup never stops agent sessions. */
export function useBindThreadToolPaneView(
  channelId?: string | null,
  rootEventId?: string | null,
) {
  const scope = useThreadToolPaneScope(channelId, rootEventId);
  React.useLayoutEffect(() => bindThreadToolPaneView(scope), [scope]);
}

/** Capture the viewer/community of a mutation, independently of the selected thread. */
export function useThreadToolPaneRemoval() {
  const { relayUrl, viewerPubkey } = useThreadToolPaneScope();
  return captureThreadToolPaneRemoval(relayUrl, viewerPubkey);
}
