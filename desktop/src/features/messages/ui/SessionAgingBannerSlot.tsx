import { useMemo } from "react";

import { useSessionAgingForConversations } from "@/features/messages/lib/useSessionAging";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { THREAD_PANEL_MESSAGE_GUTTER_CLASS } from "@/features/messages/lib/messageThreadPanelLayout";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { cn } from "@/shared/lib/cn";
import { SessionAgingBanner } from "./SessionAgingBanner";

/**
 * Thin slot under the thread head for session-aging awareness (#173).
 * Self-contained so MessageThreadPanel stays under the file-size budget.
 */
export function SessionAgingBannerSlot({
  conversationIds,
  rootEventId,
  profiles,
}: {
  conversationIds: readonly (string | null | undefined)[];
  rootEventId?: string | null;
  profiles?: UserProfileLookup;
}) {
  const entries = useSessionAgingForConversations(conversationIds);
  const agentNamesByPubkey = useMemo(() => {
    const map = new Map<string, string>();
    if (!profiles) {
      return map;
    }
    for (const [pubkey, profile] of Object.entries(profiles)) {
      const name = profile.displayName?.trim() || undefined;
      if (name) {
        map.set(normalizePubkey(pubkey), name);
      }
    }
    return map;
  }, [profiles]);

  if (entries.length === 0) {
    return null;
  }

  return (
    <div
      className={cn(THREAD_PANEL_MESSAGE_GUTTER_CLASS, "pb-2 pt-1")}
      data-testid="session-aging-banner-slot"
    >
      <SessionAgingBanner
        agentNamesByPubkey={agentNamesByPubkey}
        entries={entries}
        rootEventId={rootEventId}
      />
    </div>
  );
}
