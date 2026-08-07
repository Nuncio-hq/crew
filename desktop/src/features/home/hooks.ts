import { useQuery } from "@tanstack/react-query";

import {
  ingestApprovalRequestFeedItem,
  reconcileNeedsYouFromFeed,
} from "@/features/agents/needsYouStore";
import { getHomeFeed } from "@/shared/api/tauri";
import { useRelayConnection } from "@/shared/api/useRelayConnection";

// The native get_feed command queries approvals with a HARDCODED page bound
// of 20 (src-tauri/src/commands/messages.rs approval_filter "limit": 20),
// independent of the limit we pass for the other feed sections. Reconcile
// deletions are only safe when the page is provably complete, i.e. shorter
// than that bound.
const APPROVAL_FEED_PAGE_LIMIT = 20;

export function useHomeFeedQuery() {
  const connectionState = useRelayConnection();
  const connected = connectionState === "connected";

  return useQuery({
    queryKey: ["home-feed"],
    queryFn: async () => {
      const response = await getHomeFeed({
        limit: 50,
        types: "mentions,needs_action,activity,agent_activity",
      });
      // Hydrate the needs-you store from the relay's authoritative pending
      // set (kind 46010 needs_action, buzz-db feed.rs) and reconcile away
      // entries the fresh snapshot no longer contains (resolved while the
      // app was closed — no live grant will ever arrive for those). A full
      // page may be truncated, so deletions are gated on completeness.
      for (const item of response.feed.needsAction) {
        ingestApprovalRequestFeedItem(item);
      }
      reconcileNeedsYouFromFeed(response.feed.needsAction, Date.now(), {
        snapshotComplete:
          response.feed.needsAction.length < APPROVAL_FEED_PAGE_LIMIT,
      });
      return response;
    },
    staleTime: 15_000,
    gcTime: 5 * 60 * 1_000,
    // Pause background polling on degraded/stalled/disconnected connections.
    // The relay can't serve the request anyway, and the spurious failures
    // consume quota that the recovery path needs.
    refetchInterval: connected ? 30_000 : false,
  });
}
