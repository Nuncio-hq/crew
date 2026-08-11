/**
 * Edit-agent model block: editable picker OR profile-owned informational row.
 * Keeps runtime-id checks out of AgentInstanceEditDialog (field-model driven).
 */
import { Input } from "@/shared/ui/input";
import type { RespondToMode } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
  PERSONA_LABEL_OPTIONAL_CLASS,
} from "./agentConfigOptions";
import { PersonaDropdownField } from "./PersonaDropdownField";
import type { PersonaDropdownOption } from "./agentConfigOptions";
import {
  HermesProfileField,
  ProfileOwnedModelRow,
} from "./HermesProfileBindingFields";
import { HermesProfileModelField } from "./HermesProfileModelField";
import { HermesSoulEditor } from "./HermesSoulEditor";

export function EditAgentModelAndProfileSection({
  showProfileField,
  hermesProfile,
  onHermesProfileChange,
  modelOwnedByProfile,
  modelWriteThrough,
  personaDoc,
  disabled,
  modelRequired,
  modelDiscoveryLoading,
  modelDropdownOptions,
  modelSelectValue,
  onModelValueChange,
  showCustomModelInput,
  model,
  onCustomModelChange,
  modelStatusMessage,
  respondTo,
  editingPubkey = null,
  currentAgentName = null,
}: {
  showProfileField: boolean;
  hermesProfile: string;
  onHermesProfileChange: (next: string) => void;
  modelOwnedByProfile: boolean;
  modelWriteThrough: boolean;
  personaDoc: "soulMd" | "none";
  disabled: boolean;
  modelRequired: boolean;
  modelDiscoveryLoading: boolean;
  modelDropdownOptions: PersonaDropdownOption[];
  modelSelectValue: string;
  onModelValueChange: (value: string) => void;
  showCustomModelInput: boolean;
  model: string;
  onCustomModelChange: (next: string) => void;
  modelStatusMessage: string | null;
  respondTo?: RespondToMode | null;
  editingPubkey?: string | null;
  currentAgentName?: string | null;
}) {
  return (
    <>
      {showProfileField ? (
        <HermesProfileField
          disabled={disabled}
          currentAgentName={currentAgentName}
          editingPubkey={editingPubkey}
          id="edit-agent-hermes-profile"
          onChange={onHermesProfileChange}
          respondTo={respondTo}
          value={hermesProfile}
        />
      ) : null}

      {modelWriteThrough && hermesProfile.trim() ? (
        <HermesProfileModelField
          disabled={disabled}
          profileName={hermesProfile}
        />
      ) : modelOwnedByProfile ? (
        <ProfileOwnedModelRow profileName={hermesProfile} />
      ) : (
        <div className="space-y-1.5">
          <label
            className="text-sm font-medium text-foreground"
            htmlFor="edit-agent-model"
          >
            Model
            {modelRequired ? (
              <span className="ml-1 text-destructive" aria-hidden="true">
                *
              </span>
            ) : (
              <span className={PERSONA_LABEL_OPTIONAL_CLASS}>Optional</span>
            )}
          </label>
          <PersonaDropdownField
            disabled={disabled || modelDiscoveryLoading}
            id="edit-agent-model"
            onValueChange={onModelValueChange}
            options={modelDropdownOptions}
            placeholder="Default model"
            value={modelSelectValue}
          />
          {showCustomModelInput ? (
            <div
              className={cn(
                "mt-2 flex min-h-11 items-center px-3",
                PERSONA_FIELD_SHELL_CLASS,
              )}
            >
              <Input
                aria-label="Custom model ID"
                autoCorrect="off"
                className={cn(
                  "h-8 px-0 py-0 leading-6",
                  PERSONA_FIELD_CONTROL_CLASS,
                )}
                disabled={disabled}
                id="edit-agent-custom-model"
                onChange={(event) => onCustomModelChange(event.target.value)}
                placeholder="Custom model ID"
                value={model}
              />
            </div>
          ) : null}
          {modelStatusMessage ? (
            <p className="text-xs text-muted-foreground">
              {modelStatusMessage}
            </p>
          ) : null}
        </div>
      )}
      {personaDoc === "soulMd" && hermesProfile.trim() ? (
        <HermesSoulEditor disabled={disabled} profileName={hermesProfile} />
      ) : null}
    </>
  );
}
