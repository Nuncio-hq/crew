import * as React from "react";

import {
  deleteManagedAgentWithRules,
  type ManagedAgentActionResult,
} from "@/features/agents/lib/managedAgentControlActions";
import type {
  AgentPersona,
  Channel,
  ManagedAgent,
  RelayAgent,
} from "@/shared/api/types";

type DeleteManagedAgentRulesContext = Omit<
  Parameters<typeof deleteManagedAgentWithRules>[0],
  "agent"
>;

type DeleteProfileManagedAgentContext = DeleteManagedAgentRulesContext;

type DeleteProfileManagedAgentsForPersonaContext =
  DeleteProfileManagedAgentContext & {
    managedAgents: readonly ManagedAgent[];
    selectedAgent?: ManagedAgent;
  };

type UseProfileAgentDeletionInput = {
  channels?: readonly Channel[];
  deleteManagedAgent: DeleteManagedAgentRulesContext["deleteManagedAgent"];
  managedAgent?: ManagedAgent;
  managedAgents?: readonly ManagedAgent[];
  getAvailability: DeleteManagedAgentRulesContext["getAvailability"];
  relayAgents?: readonly RelayAgent[];
};

export function useProfileAgentDeletion({
  channels,
  deleteManagedAgent,
  managedAgent,
  managedAgents,
  getAvailability,
  relayAgents,
}: UseProfileAgentDeletionInput) {
  const deleteManagedAgentRecord = React.useCallback(
    (agentToDelete: ManagedAgent) =>
      deleteProfileManagedAgent(agentToDelete, {
        channels: channels ?? [],
        deleteManagedAgent,
        getAvailability,
        relayAgents: relayAgents ?? [],
        skipRemoteDeleteConfirm: true,
      }),
    [channels, deleteManagedAgent, getAvailability, relayAgents],
  );

  const deleteManagedAgentsForPersona = React.useCallback(
    (persona: AgentPersona) =>
      deleteProfileManagedAgentsForPersona(persona, {
        channels: channels ?? [],
        deleteManagedAgent,
        managedAgents: managedAgents ?? [],
        getAvailability,
        relayAgents: relayAgents ?? [],
        selectedAgent: managedAgent,
      }),
    [
      channels,
      deleteManagedAgent,
      managedAgent,
      managedAgents,
      getAvailability,
      relayAgents,
    ],
  );

  return {
    deleteManagedAgentRecord,
    deleteManagedAgentsForPersona,
  };
}

export async function deleteProfileManagedAgent(
  agent: ManagedAgent,
  context: DeleteProfileManagedAgentContext,
): Promise<ManagedAgentActionResult> {
  const result = await deleteManagedAgentWithRules({
    agent,
    ...context,
  });
  return result;
}

export async function deleteProfileManagedAgentsForPersona(
  persona: AgentPersona,
  context: DeleteProfileManagedAgentsForPersonaContext,
): Promise<ManagedAgentActionResult> {
  const { managedAgents, selectedAgent, ...deleteContext } = context;
  const agentsByPubkey = new Map<string, ManagedAgent>();

  for (const agent of managedAgents) {
    if (agent.personaId === persona.id) {
      agentsByPubkey.set(agent.pubkey, agent);
    }
  }

  if (selectedAgent?.personaId === persona.id) {
    agentsByPubkey.set(selectedAgent.pubkey, selectedAgent);
  }

  for (const agent of agentsByPubkey.values()) {
    const result = await deleteProfileManagedAgent(agent, deleteContext);
    if (result.cancelled) return result;
  }

  return {};
}
