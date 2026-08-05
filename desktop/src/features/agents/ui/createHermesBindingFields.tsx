/**
 * Create-flow Hermes binding + profile-owned model row.
 * Extracted so AgentDefinitionDialog stays under the file-size ratchet.
 */
import * as React from "react";

import type { AcpRuntimeCatalogEntry } from "@/shared/api/types";
import {
  deriveAgentConfigFieldModel,
  getRenderableHermesProfileField,
  isModelOwnedByProfile,
} from "../lib/agentConfigCore";
import { hermesProfileBindingError } from "../lib/hermesProfileBinding";
import { EMPTY_GLOBAL_CONFIG } from "./AgentConfigFields";
import {
  HermesProfileField,
  ProfileOwnedModelRow,
} from "./HermesProfileBindingFields";

export function useCreateHermesBinding({
  enabled,
  hermesProfile,
  runtime,
}: {
  enabled: boolean;
  hermesProfile: string;
  runtime: AcpRuntimeCatalogEntry | undefined;
}) {
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
  const profileError = showProfileField
    ? hermesProfileBindingError(hermesProfile, true)
    : null;
  return { showProfileField, modelOwnedByProfile, profileError };
}

export function CreateHermesBindingFields({
  disabled,
  hermesProfile,
  onHermesProfileChange,
  modelOwnedByProfile,
  showProfileField,
  respondTo,
}: {
  disabled?: boolean;
  hermesProfile: string;
  onHermesProfileChange: (next: string) => void;
  modelOwnedByProfile: boolean;
  showProfileField: boolean;
  respondTo?: string | null;
}) {
  if (!showProfileField && !modelOwnedByProfile) return null;
  return (
    <>
      {showProfileField ? (
        <HermesProfileField
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
