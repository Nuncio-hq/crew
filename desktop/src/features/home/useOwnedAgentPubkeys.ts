import * as React from "react";

import {
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { mergeOwnedAgentPubkeys } from "@/features/agents/knownAgentPubkeys";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import type { UserProfileLookup } from "@/features/profile/lib/identity";

export function useOwnedAgentPubkeys(
  enabled: boolean,
  profiles: UserProfileLookup | undefined,
  currentPubkey: string | undefined,
) {
  const managedAgents = useManagedAgentsQuery({ enabled }).data;
  return React.useMemo(
    () => mergeOwnedAgentPubkeys(managedAgents, profiles, currentPubkey),
    [currentPubkey, managedAgents, profiles],
  );
}

export function useCurrentOwnedAgentPubkeys(currentPubkey: string | undefined) {
  const relayAgents = useRelayAgentsQuery({
    enabled: Boolean(currentPubkey),
  }).data;
  const relayAgentPubkeys = React.useMemo(
    () => (relayAgents ?? []).map((agent) => agent.pubkey),
    [relayAgents],
  );
  const profiles = useUsersBatchQuery(relayAgentPubkeys, {
    enabled: Boolean(currentPubkey) && relayAgentPubkeys.length > 0,
  }).data?.profiles;
  return useOwnedAgentPubkeys(Boolean(currentPubkey), profiles, currentPubkey);
}
