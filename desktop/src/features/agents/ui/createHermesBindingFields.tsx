/**
 * Create/edit Hermes binding + profile-owned model row.
 * Extracted so AgentDefinitionDialog stays under the file-size ratchet.
 *
 * Visibility follows the runtime catalog (`profileArg` / write-through), not
 * create-vs-edit. Occupancy treats a linked instance as self so Edit agent
 * can keep its already-bound profile.
 */
import * as React from "react";

import type { AcpRuntimeCatalogEntry, RespondToMode } from "@/shared/api/types";
import { useManagedAgentsQuery } from "../hooks";
import {
  deriveAgentConfigFieldModel,
  getRenderableHermesProfileField,
  isModelWriteThrough,
  isModelOwnedByProfile,
} from "../lib/agentConfigCore";
import {
  resolveHermesProfileForCreate,
  resolveHermesProfileForUpdate,
  shouldClearHermesProfileOnRuntimeChange,
} from "../lib/hermesProfileBinding";
import { EMPTY_GLOBAL_CONFIG } from "./AgentConfigFields";
import {
  HermesProfileField,
  ProfileOwnedModelRow,
  useHermesProfileBindingState,
} from "./HermesProfileBindingFields";
import { HermesProfileModelField } from "./HermesProfileModelField";
import { HermesSoulEditor } from "./HermesSoulEditor";

export function useCreateHermesBinding({
  personaId = null,
  hermesProfile,
  onHermesProfileChange,
  respondTo,
  runtime,
}: {
  /** Definition id while editing; null/absent on create. */
  personaId?: string | null;
  hermesProfile: string;
  onHermesProfileChange: (next: string) => void;
  respondTo: RespondToMode | null;
  runtime: AcpRuntimeCatalogEntry | undefined;
}) {
  const isCreate = !personaId?.trim();
  const agentsQuery = useManagedAgentsQuery({ enabled: !isCreate });
  const linkedAgent = React.useMemo(() => {
    const id = personaId?.trim();
    if (!id) return null;
    return (
      (agentsQuery.data ?? []).find((agent) => agent.personaId === id) ?? null
    );
  }, [agentsQuery.data, personaId]);

  React.useEffect(() => {
    if (hermesProfile && shouldClearHermesProfileOnRuntimeChange(runtime)) {
      onHermesProfileChange("");
    }
  }, [hermesProfile, onHermesProfileChange, runtime]);

  React.useEffect(() => {
    if (shouldClearHermesProfileOnRuntimeChange(runtime)) return;
    const bound = linkedAgent?.hermesProfile?.trim() ?? "";
    if (!bound || hermesProfile.trim()) return;
    onHermesProfileChange(bound);
  }, [
    hermesProfile,
    linkedAgent?.hermesProfile,
    onHermesProfileChange,
    runtime,
  ]);

  const fieldModel = React.useMemo(
    () =>
      deriveAgentConfigFieldModel({
        config: EMPTY_GLOBAL_CONFIG,
        hermesProfile,
        runtime,
        scope: isCreate ? "definition" : "instance",
      }),
    [hermesProfile, isCreate, runtime],
  );
  const offersBinding = getRenderableHermesProfileField(fieldModel) != null;
  const showProfileField = offersBinding && (isCreate || linkedAgent != null);
  const modelOwnedByProfile = isModelOwnedByProfile(fieldModel);
  const modelWriteThrough = isModelWriteThrough(fieldModel);
  const { blockingError } = useHermesProfileBindingState({
    currentAgentName: linkedAgent?.name ?? null,
    editingPubkey: linkedAgent?.pubkey ?? null,
    enabled: showProfileField,
    hermesProfile,
    required: showProfileField,
    respondTo,
  });
  return {
    showProfileField,
    modelOwnedByProfile,
    modelWriteThrough,
    profileError: blockingError,
    hermesProfileForSubmit: isCreate
      ? resolveHermesProfileForCreate(hermesProfile, runtime)
      : resolveHermesProfileForUpdate(
          linkedAgent?.hermesProfile,
          hermesProfile,
          runtime,
        ),
  };
}

export function CreateHermesBindingFields({
  currentAgentName,
  disabled,
  hermesProfile,
  onHermesProfileChange,
  modelOwnedByProfile,
  modelWriteThrough,
  personaDoc,
  showProfileField,
  respondTo,
}: {
  currentAgentName?: string | null;
  disabled?: boolean;
  hermesProfile: string;
  onHermesProfileChange: (next: string) => void;
  modelOwnedByProfile: boolean;
  modelWriteThrough: boolean;
  personaDoc: "soulMd" | "none";
  showProfileField: boolean;
  respondTo?: RespondToMode | null;
}) {
  if (!showProfileField && !modelOwnedByProfile && !modelWriteThrough) {
    return null;
  }
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
      {modelWriteThrough && hermesProfile.trim() ? (
        <HermesProfileModelField profileName={hermesProfile} />
      ) : modelOwnedByProfile ? (
        <ProfileOwnedModelRow profileName={hermesProfile} />
      ) : null}
      {personaDoc === "soulMd" && hermesProfile.trim() ? (
        <HermesSoulEditor profileName={hermesProfile} />
      ) : null}
    </>
  );
}
