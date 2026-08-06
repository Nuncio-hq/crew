/**
 * Edit-flow Hermes binding visibility + save-gate error (Phase 04 occupancy).
 */
import type { ManagedAgent } from "@/shared/api/types";
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
}: {
  agent: Pick<ManagedAgent, "pubkey">;
  fieldModel: AgentConfigFieldModel;
  hermesProfile: string;
}) {
  const showProfileField = getRenderableHermesProfileField(fieldModel) != null;
  const modelOwnedByProfile = isModelOwnedByProfile(fieldModel);
  const { profileError } = useHermesProfileBindingState({
    editingPubkey: agent.pubkey,
    enabled: showProfileField,
    hermesProfile,
  });
  return {
    showHermesProfileField: showProfileField,
    modelOwnedByProfile,
    hermesProfileError: profileError,
  };
}
