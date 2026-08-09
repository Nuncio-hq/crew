import * as React from "react";

import {
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { mergeOwnedAgentPubkeys } from "@/features/agents/knownAgentPubkeys";
import { useUsersBatchQuery } from "@/features/profile/hooks";

export function useCurrentOwnedAgentPubkeys(currentPubkey: string | undefined) {
  const managedAgents = useManagedAgentsQuery({
    enabled: Boolean(currentPubkey),
  }).data;
  const relayAgents = useRelayAgentsQuery({
    enabled: Boolean(currentPubkey),
  }).data;
  const candidateAgentPubkeys = React.useMemo(
    () => [
      ...new Set([
        ...(managedAgents ?? []).map((agent) => agent.pubkey),
        ...(relayAgents ?? []).map((agent) => agent.pubkey),
      ]),
    ],
    [managedAgents, relayAgents],
  );
  const profiles = useUsersBatchQuery(candidateAgentPubkeys, {
    enabled: Boolean(currentPubkey) && candidateAgentPubkeys.length > 0,
  }).data?.profiles;
  return React.useMemo(
    () => mergeOwnedAgentPubkeys(profiles, currentPubkey),
    [currentPubkey, profiles],
  );
}
