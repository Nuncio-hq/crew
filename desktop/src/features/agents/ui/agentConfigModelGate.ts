/**
 * Model control visibility: optional-discovery omit + profile-owned omission.
 * Split from AgentConfigFields for the file-size / biome-format ratchet.
 */
import type { AgentConfigFieldModel } from "../lib/agentConfigCore";
import { isModelOwnedByProfile } from "../lib/agentConfigCore";

/**
 * Renders the Model control given discovery state. Optional-model harnesses omit it while
 * discovery is loading or after confirmed successful empty; failures keep it for the #2246 UI.
 */
export function shouldRenderModelControl({
  discoveredModelOptions,
  modelDiscoveryLoading,
  modelDiscoverySuccessfulEmpty,
  modelIsOptional,
  showCustomModelOption,
}: {
  discoveredModelOptions: readonly { id: string }[] | null;
  modelDiscoveryLoading: boolean;
  /** True only when discovery IPC resolved with a response that yielded no options. */
  modelDiscoverySuccessfulEmpty: boolean;
  modelIsOptional: boolean;
  showCustomModelOption: boolean;
}): boolean {
  if (!modelIsOptional) return true;
  if (modelDiscoveryLoading) return false;
  const hasExplicitModel = (discoveredModelOptions ?? []).some(
    (option) => option.id.trim().length > 0,
  );
  if (hasExplicitModel) return true;
  if (showCustomModelOption) return true;
  // Omit only on confirmed successful empty — not on failure/unavailable.
  return !modelDiscoverySuccessfulEmpty;
}

export function resolveAgentConfigModelGate({
  fieldModel,
  configModel,
  fallbackModel,
  dependentFieldsDisabled,
  discoveredModelOptions,
  modelDiscoveryLoading,
  modelDiscoverySuccessfulEmpty,
  showCustomModelOption,
}: {
  fieldModel: AgentConfigFieldModel;
  configModel: string | null | undefined;
  fallbackModel: string | null;
  dependentFieldsDisabled: boolean;
  discoveredModelOptions: readonly { id: string }[] | null;
  modelDiscoveryLoading: boolean;
  modelDiscoverySuccessfulEmpty: boolean;
  showCustomModelOption: boolean;
}): {
  modelOwnedByProfile: boolean;
  modelIsOptional: boolean;
  modelIsValid: boolean;
  modelControlVisible: boolean;
} {
  const modelField = fieldModel.fields.find(
    (field) => field.kind === "model" && field.render === "control",
  );
  const modelOwnedByProfile = isModelOwnedByProfile(fieldModel);
  const modelIsOptional = modelField?.targetApplication.kind === "acpNative";
  const modelIsValid =
    modelOwnedByProfile ||
    modelIsOptional ||
    (configModel?.trim().length ?? 0) > 0 ||
    fallbackModel !== null;
  const modelControlVisible =
    !modelOwnedByProfile &&
    shouldRenderModelControl({
      discoveredModelOptions: dependentFieldsDisabled
        ? null
        : discoveredModelOptions,
      modelDiscoveryLoading: dependentFieldsDisabled
        ? false
        : modelDiscoveryLoading,
      modelDiscoverySuccessfulEmpty:
        !dependentFieldsDisabled && modelDiscoverySuccessfulEmpty,
      modelIsOptional,
      showCustomModelOption,
    });
  return {
    modelOwnedByProfile,
    modelIsOptional,
    modelIsValid,
    modelControlVisible,
  };
}
