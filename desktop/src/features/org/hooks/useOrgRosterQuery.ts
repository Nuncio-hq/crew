import { useQuery } from "@tanstack/react-query";
import * as React from "react";

import {
  getOrgRosterProjection,
  setOrgRosterProjection,
  subscribeOrgRosterProjection,
} from "@/features/org/lib/orgProjectionStore";
import { parseOrgRoster, type OrgRoster } from "@/features/org/lib/orgRoster";
import { relayClient } from "@/shared/api/relayClient";
import { KIND_ORG_ROSTER } from "@/shared/constants/kinds";
import { useIdentityQuery } from "@/shared/api/hooks";

export const orgRosterQueryKey = ["org-roster"] as const;

export function useOrgRosterQuery() {
  const identity = useIdentityQuery();
  const founderFallback = identity.data?.pubkey ?? "";
  const query = useQuery({
    queryKey: orgRosterQueryKey,
    queryFn: async (): Promise<OrgRoster | null> => {
      const events = await relayClient.fetchEvents({
        kinds: [KIND_ORG_ROSTER],
        "#d": ["org"],
        limit: 5,
      });
      if (events.length === 0) {
        setOrgRosterProjection(null);
        return null;
      }
      const latest = [...events].sort((left, right) => {
        if (left.created_at !== right.created_at) {
          return right.created_at - left.created_at;
        }
        return right.id.localeCompare(left.id);
      })[0];
      const parsed = parseOrgRoster(latest.content, latest.pubkey);
      if ("error" in parsed) {
        setOrgRosterProjection(null);
        return null;
      }
      const roster: OrgRoster = {
        ...parsed.roster,
        eventId: latest.id,
        createdAt: latest.created_at,
      };
      setOrgRosterProjection(roster);
      return roster;
    },
    staleTime: 15_000,
  });
  const projected = React.useSyncExternalStore(
    subscribeOrgRosterProjection,
    getOrgRosterProjection,
    getOrgRosterProjection,
  );
  return {
    ...query,
    data: projected ?? query.data,
    founderPubkey:
      projected?.founderPubkey ?? query.data?.founderPubkey ?? founderFallback,
  };
}
