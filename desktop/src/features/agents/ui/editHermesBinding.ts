/**
 * Edit-flow Hermes binding visibility + save-gate error (Phase 04 occupancy).
 */
import type { ManagedAgent, RespondToMode } from "@/shared/api/types";
import type { AgentConfigFieldModel } from "../lib/agentConfigCore";
import {
  getRenderableHermesProfileField,
  isModelOwnedByProfile,
} from "../lib/agentConfigCore";
import { useHermesProfileBindingState } from "./HermesProfileBindingFields";

export function useEditHermesBinding({
  agent,
  fieldModel,
  hermesProfile,
  respondTo,
}: {
  agent: Pick<ManagedAgent, "name" | "pubkey">;
  fieldModel: AgentConfigFieldModel;
  hermesProfile: string;
  respondTo: RespondToMode;
}) {
  const showProfileField = getRenderableHermesProfileField(fieldModel) != null;
  const modelOwnedByProfile = isModelOwnedByProfile(fieldModel);
  const { blockingError } = useHermesProfileBindingState({
    currentAgentName: agent.name,
    editingPubkey: agent.pubkey,
    enabled: showProfileField,
    hermesProfile,
    respondTo,
  });
  return {
    showHermesProfileField: showProfileField,
    modelOwnedByProfile,
    hermesProfileError: blockingError,
  };
}
