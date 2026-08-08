/**
 * Create-flow Hermes binding + profile-owned model row.
 * Extracted so AgentDefinitionDialog stays under the file-size ratchet.
 */
import * as React from "react";

import type { AcpRuntimeCatalogEntry, RespondToMode } from "@/shared/api/types";
import {
  deriveAgentConfigFieldModel,
  getRenderableHermesProfileField,
  isModelOwnedByProfile,
} from "../lib/agentConfigCore";
import { shouldClearHermesProfileOnRuntimeChange } from "../lib/hermesProfileBinding";
import { EMPTY_GLOBAL_CONFIG } from "./AgentConfigFields";
import {
  HermesProfileField,
  ProfileOwnedModelRow,
  useHermesProfileBindingState,
} from "./HermesProfileBindingFields";

export function useCreateHermesBinding({
  enabled,
  hermesProfile,
  onHermesProfileChange,
  respondTo,
  runtime,
}: {
  enabled: boolean;
  hermesProfile: string;
  onHermesProfileChange: (next: string) => void;
  respondTo: RespondToMode | null;
  runtime: AcpRuntimeCatalogEntry | undefined;
}) {
  React.useEffect(() => {
    if (hermesProfile && shouldClearHermesProfileOnRuntimeChange(runtime)) {
      onHermesProfileChange("");
    }
  }, [hermesProfile, onHermesProfileChange, runtime]);
  const fieldModel = React.useMemo(
    () =>
      deriveAgentConfigFieldModel({
        config: EMPTY_GLOBAL_CONFIG,
        hermesProfile,
        runtime,
        scope: "instance",
      }),
    [hermesProfile, runtime],
  );
  const showProfileField =
    enabled && getRenderableHermesProfileField(fieldModel) != null;
  const modelOwnedByProfile = enabled && isModelOwnedByProfile(fieldModel);
  const { blockingError } = useHermesProfileBindingState({
    enabled: showProfileField,
    hermesProfile,
    editingPubkey: null,
    respondTo,
    required: true,
  });
  return {
    showProfileField,
    modelOwnedByProfile,
    profileError: blockingError,
  };
}

export function CreateHermesBindingFields({
  currentAgentName,
  disabled,
  hermesProfile,
  onHermesProfileChange,
  modelOwnedByProfile,
  showProfileField,
  respondTo,
}: {
  currentAgentName?: string | null;
  disabled?: boolean;
  hermesProfile: string;
  onHermesProfileChange: (next: string) => void;
  modelOwnedByProfile: boolean;
  showProfileField: boolean;
  respondTo?: RespondToMode | null;
}) {
  if (!showProfileField && !modelOwnedByProfile) return null;
  return (
    <>
      {showProfileField ? (
        <HermesProfileField
          currentAgentName={currentAgentName}
          disabled={disabled}
          id="persona-hermes-profile"
          onChange={onHermesProfileChange}
          respondTo={respondTo}
          value={hermesProfile}
        />
      ) : null}
      {modelOwnedByProfile ? (
        <ProfileOwnedModelRow profileName={hermesProfile} />
      ) : null}
    </>
  );
}
