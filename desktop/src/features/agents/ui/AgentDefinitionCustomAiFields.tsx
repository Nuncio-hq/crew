import type * as React from "react";
import { AnimatePresence, type motion } from "motion/react";

import type { RespondToMode } from "@/shared/api/types";
import { Input } from "@/shared/ui/input";
import { cn } from "@/shared/lib/cn";
import {
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
  PERSONA_LABEL_OPTIONAL_CLASS,
  type PersonaDropdownOption,
} from "./agentConfigOptions";
import { RequiredFieldLabel } from "./agentConfigControls";
import { PersonaDropdownField } from "./PersonaDropdownField";
import { PersonaProviderApiKeyField } from "./PersonaProviderApiKeyField";
import { PersonaModelField } from "./PersonaModelField";
import { CreateHermesBindingFields } from "./createHermesBindingFields";
import type { AgentAiConfigurationMode } from "./AgentAiConfigurationMode";
import type { PersonaModelDiscoveryStatus } from "./personaModelDiscoveryStatus";

export function AgentDefinitionCustomAiFields({
  aiConfigurationMode,
  currentAgentName,
  disabled,
  effectiveProviderApiKeyLabel,
  hermesProfile,
  isExplicitModelRequired,
  isRelayMesh,
  llmProviderFieldVisible,
  model,
  modelDiscoveryStatus,
  modelDropdownOptions,
  modelFieldVisible,
  modelOwnedByProfile,
  modelSelectValue,
  onCustomModelChange,
  onHermesProfileChange,
  onModelValueChange,
  onProviderChange,
  onProviderValueChange,
  onSecretEnvChange,
  provider,
  providerDropdownOptions,
  providerIsRequired,
  providerSelectValue,
  showCustomModelInput,
  showCustomProviderInput,
  showHermesProfileField,
  respondTo,
  topLevelSecretEnvVar,
  apiKeyInheritedLabel,
  apiKeyIsInherited,
  apiKeyIsRequired,
  apiKeyValue,
  autoModelDropdownValue,
  transition,
}: {
  aiConfigurationMode: AgentAiConfigurationMode;
  currentAgentName?: string | null;
  disabled: boolean;
  effectiveProviderApiKeyLabel: string;
  hermesProfile: string;
  isExplicitModelRequired: boolean;
  isRelayMesh: boolean;
  llmProviderFieldVisible: boolean;
  model: string;
  modelDiscoveryStatus: PersonaModelDiscoveryStatus | null;
  modelDropdownOptions: PersonaDropdownOption[];
  modelFieldVisible: boolean;
  modelOwnedByProfile: boolean;
  modelSelectValue: string;
  onCustomModelChange: (next: string) => void;
  onHermesProfileChange: (next: string) => void;
  onModelValueChange: (value: string) => void;
  onProviderChange: (next: string) => void;
  onProviderValueChange: (value: string) => void;
  onSecretEnvChange: (next: string) => void;
  provider: string;
  providerDropdownOptions: PersonaDropdownOption[];
  providerIsRequired: boolean;
  providerSelectValue: string;
  showCustomModelInput: boolean;
  showCustomProviderInput: boolean;
  showHermesProfileField: boolean;
  respondTo?: RespondToMode | null;
  topLevelSecretEnvVar: string | null;
  apiKeyInheritedLabel: string | null;
  apiKeyIsInherited: boolean;
  apiKeyIsRequired: boolean;
  apiKeyValue: string;
  autoModelDropdownValue: string;
  transition: React.ComponentProps<typeof motion.div>["transition"];
}) {
  return (
    <>
      {llmProviderFieldVisible && aiConfigurationMode === "custom" ? (
        <div className="space-y-1.5">
          <RequiredFieldLabel
            htmlFor="persona-llm-provider"
            isRequired={providerIsRequired}
          >
            LLM provider
            {!providerIsRequired ? (
              <span className={PERSONA_LABEL_OPTIONAL_CLASS}>Optional</span>
            ) : null}
          </RequiredFieldLabel>
          <PersonaDropdownField
            disabled={disabled}
            id="persona-llm-provider"
            onValueChange={onProviderValueChange}
            options={providerDropdownOptions}
            placeholder="Choose a provider"
            value={providerSelectValue}
          />
          {showCustomProviderInput ? (
            <div
              className={cn(
                "mt-2 flex min-h-11 items-center px-3",
                PERSONA_FIELD_SHELL_CLASS,
              )}
            >
              <Input
                aria-label="Custom provider ID"
                autoCorrect="off"
                className={cn(
                  "h-8 px-0 py-0 leading-6",
                  PERSONA_FIELD_CONTROL_CLASS,
                )}
                disabled={disabled}
                id="persona-custom-provider"
                onChange={(event) => onProviderChange(event.target.value)}
                placeholder="Custom provider ID"
                value={provider}
              />
            </div>
          ) : null}
        </div>
      ) : null}

      {llmProviderFieldVisible &&
      aiConfigurationMode === "custom" &&
      topLevelSecretEnvVar ? (
        <PersonaProviderApiKeyField
          disabled={disabled}
          envVarName={topLevelSecretEnvVar}
          isInherited={apiKeyIsInherited}
          inheritedLabel={apiKeyInheritedLabel ?? "Inherited"}
          isRequired={apiKeyIsRequired}
          label={effectiveProviderApiKeyLabel}
          onValueChange={onSecretEnvChange}
          value={apiKeyValue}
        />
      ) : null}

      <AnimatePresence initial={false}>
        {modelFieldVisible && aiConfigurationMode === "custom" ? (
          <PersonaModelField
            disabled={disabled}
            isExplicitModelRequired={isExplicitModelRequired}
            model={model}
            modelDiscoveryStatus={modelDiscoveryStatus}
            modelDropdownOptions={modelDropdownOptions}
            modelSelectValue={modelSelectValue}
            onCustomModelChange={onCustomModelChange}
            showSharedComputeAutoHint={
              isRelayMesh && modelSelectValue === autoModelDropdownValue
            }
            onModelValueChange={onModelValueChange}
            showCustomModelInput={showCustomModelInput}
            transition={transition}
          />
        ) : null}
      </AnimatePresence>

      <CreateHermesBindingFields
        currentAgentName={currentAgentName}
        disabled={disabled}
        hermesProfile={hermesProfile}
        modelOwnedByProfile={modelOwnedByProfile}
        onHermesProfileChange={onHermesProfileChange}
        respondTo={respondTo}
        showProfileField={showHermesProfileField}
      />
    </>
  );
}
