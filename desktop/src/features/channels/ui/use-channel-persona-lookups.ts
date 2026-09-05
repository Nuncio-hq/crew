import * as React from "react";
import { usePersonasQuery } from "@/features/agents/hooks";
import {
  personaAvatarById,
  personaRuntimeById,
} from "@/features/agents/lib/agentCardAvatar";
import type { ManagedAgent, RespondToMode } from "@/shared/api/types";

/** Derive profile presentation and policy lookups from the same persona snapshot. */
export function useChannelPersonaLookups(
  managedAgents: ManagedAgent[] | undefined,
) {
  const personasQuery = usePersonasQuery();
  const personaAvatars = React.useMemo(
    () => personaAvatarById(personasQuery.data ?? []),
    [personasQuery.data],
  );
  const personaRuntimes = React.useMemo(
    () => personaRuntimeById(personasQuery.data ?? []),
    [personasQuery.data],
  );
  const { personaLookup, respondToLookup } = React.useMemo(() => {
    const agents = managedAgents ?? [];
    const personaById = new Map(
      (personasQuery.data ?? []).map((p) => [p.id, p.displayName]),
    );
    const pLookup = new Map<string, string>();
    const rLookup = new Map<string, RespondToMode>();
    for (const agent of agents) {
      const key = agent.pubkey.toLowerCase();
      rLookup.set(key, agent.respondTo);
      const pName = agent.personaId ? personaById.get(agent.personaId) : null;
      if (pName) pLookup.set(key, pName);
    }
    return { personaLookup: pLookup, respondToLookup: rLookup };
  }, [managedAgents, personasQuery.data]);
  return { personaAvatars, personaRuntimes, personaLookup, respondToLookup };
}
