import { useMutation, useQueryClient } from "@tanstack/react-query";

import { orgRosterQueryKey } from "@/features/org/hooks/useOrgRosterQuery";
import { setOrgRosterProjection } from "@/features/org/lib/orgProjectionStore";
import {
  serializeOrgRoster,
  type OrgRoster,
} from "@/features/org/lib/orgRoster";
import { relayClient } from "@/shared/api/relayClient";
import { signRelayEvent } from "@/shared/api/tauri";
import { KIND_ORG_ROSTER } from "@/shared/constants/kinds";

export function usePublishOrgRoster() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (roster: OrgRoster) => {
      const event = await signRelayEvent({
        kind: KIND_ORG_ROSTER,
        content: serializeOrgRoster(roster),
        tags: [["d", "org"]],
      });
      await relayClient.publishEvent(
        event,
        "Timed out publishing the org roster.",
        "Failed to publish the org roster.",
      );
      const published: OrgRoster = {
        ...roster,
        eventId: event.id,
        createdAt: event.created_at,
        founderPubkey: event.pubkey.toLowerCase(),
      };
      setOrgRosterProjection(published);
      return published;
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: orgRosterQueryKey });
    },
  });
}
