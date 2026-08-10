/**
 * Hermes-aware delete handlers for the user profile panel (Phase 03).
 */
import * as React from "react";
import { toast } from "sonner";

import { archiveHermesProfilesAfterAgentRemoval } from "@/features/profile/ui/archiveHermesProfilesAfterAgentRemoval";
import type { AgentPersona, ManagedAgent } from "@/shared/api/types";
import type { ManagedAgentActionResult } from "@/features/agents/lib/managedAgentControlActions";

export function useProfileHermesAwareDeletes({
  managedAgent,
  managedAgents,
  deleteManagedAgentRecord,
  deletePersona,
  onClose,
  setPersonaToDelete,
}: {
  managedAgent?: ManagedAgent;
  managedAgents: readonly ManagedAgent[];
  deleteManagedAgentRecord: (
    agent: ManagedAgent,
  ) => Promise<ManagedAgentActionResult>;
  deletePersona: (id: string) => Promise<unknown>;
  onClose: () => void;
  setPersonaToDelete: (persona: AgentPersona | null) => void;
}) {
  const handleDeleteAgent = React.useCallback(
    async (options?: {
      archiveHermesProfile?: boolean;
      hermesProfileReason?: string;
    }) => {
      if (!managedAgent) return;
      try {
        const profileToDelete =
          options?.archiveHermesProfile === true
            ? managedAgent.hermesProfile?.trim() || null
            : null;
        const result = await deleteManagedAgentRecord(managedAgent);
        if (result.cancelled) return;
        if (profileToDelete) {
          const profileError = await archiveHermesProfilesAfterAgentRemoval([
            profileToDelete,
          ]);
          if (profileError) {
            toast.error(
              `Agent deleted, but profile cleanup failed: ${profileError}`,
            );
            onClose();
            return;
          }
        }
        toast.success(`Deleted ${managedAgent.name}.`);
        onClose();
      } catch (error) {
        toast.error(
          error instanceof Error ? error.message : "Failed to delete agent.",
        );
      }
    },
    [deleteManagedAgentRecord, managedAgent, onClose],
  );

  const handleConfirmDeletePersona = React.useCallback(
    async (
      personaToConfirm: AgentPersona,
      options?: {
        archiveHermesProfiles?: boolean;
        hermesProfileReason?: string;
      },
    ) => {
      if (personaToConfirm.sourceTeam) {
        toast.error("This agent is managed by a team.");
        setPersonaToDelete(null);
        return;
      }
      try {
        const profilesToDelete =
          options?.archiveHermesProfiles === true
            ? managedAgents
                .filter((a) => a.personaId === personaToConfirm.id)
                .map((a) => a.hermesProfile?.trim())
                .filter((p): p is string => Boolean(p))
            : [];
        await deletePersona(personaToConfirm.id);
        if (profilesToDelete.length > 0) {
          const profileError =
            await archiveHermesProfilesAfterAgentRemoval(profilesToDelete);
          if (profileError) {
            toast.error(
              `Deleted ${personaToConfirm.displayName}, but profile cleanup failed: ${profileError}`,
            );
            setPersonaToDelete(null);
            onClose();
            return;
          }
        }
        toast.success(`Deleted ${personaToConfirm.displayName}.`);
        setPersonaToDelete(null);
        onClose();
      } catch (error) {
        toast.error(
          error instanceof Error ? error.message : "Failed to delete agent.",
        );
      }
    },
    [deletePersona, managedAgents, onClose, setPersonaToDelete],
  );

  return { handleDeleteAgent, handleConfirmDeletePersona };
}

export function hermesProfilesForPersona(
  managedAgents: readonly ManagedAgent[],
  personaId: string | undefined,
): string[] {
  if (!personaId) return [];
  return managedAgents
    .filter((a) => a.personaId === personaId)
    .map((a) => a.hermesProfile?.trim())
    .filter((p): p is string => Boolean(p));
}
