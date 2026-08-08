/**
 * Edit-flow Hermes binding visibility + save-gate error (Phase 04 occupancy).
 */
import * as React from "react";
import type {
  AcpRuntimeCatalogEntry,
  ManagedAgent,
  RespondToMode,
} from "@/shared/api/types";
import type { AgentConfigFieldModel } from "../lib/agentConfigCore";
import {
  getRenderableHermesProfileField,
  isModelOwnedByProfile,
} from "../lib/agentConfigCore";
import {
  resolveHermesProfileForUpdate,
  shouldClearHermesProfileOnRuntimeChange,
} from "../lib/hermesProfileBinding";
import { useHermesProfileBindingState } from "./HermesProfileBindingFields";

export function useEditHermesBinding({
  agent,
  fieldModel,
  hermesProfile,
  onHermesProfileChange,
  respondTo,
  runtime,
  runtimeSelectionConfirmedNoBinding,
}: {
  agent: Pick<ManagedAgent, "hermesProfile" | "name" | "pubkey">;
  fieldModel: AgentConfigFieldModel;
  hermesProfile: string;
  onHermesProfileChange: (next: string) => void;
  respondTo: RespondToMode;
  runtime: AcpRuntimeCatalogEntry | undefined;
  runtimeSelectionConfirmedNoBinding: boolean;
}) {
  const showProfileField = getRenderableHermesProfileField(fieldModel) != null;
  const modelOwnedByProfile = isModelOwnedByProfile(fieldModel);
  React.useEffect(() => {
    if (
      hermesProfile &&
      shouldClearHermesProfileOnRuntimeChange(
        runtime,
        runtimeSelectionConfirmedNoBinding,
      )
    ) {
      onHermesProfileChange("");
    }
  }, [
    hermesProfile,
    onHermesProfileChange,
    runtime,
    runtimeSelectionConfirmedNoBinding,
  ]);
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
    hermesProfileForSubmit: resolveHermesProfileForUpdate(
      agent.hermesProfile,
      hermesProfile,
      runtime,
      runtimeSelectionConfirmedNoBinding,
    ),
  };
}
