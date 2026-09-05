import type { ReactNode } from "react";
import { cn } from "@/shared/lib/cn";
import { Input } from "@/shared/ui/input";

import {
  AUTO_PROVIDER_DROPDOWN_VALUE,
  BLOCK_BUILD_HIDDEN_PROVIDER_IDS,
  CUSTOM_PROVIDER_DROPDOWN_VALUE,
  getPersonaProviderOptions,
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
  PERSONA_LABEL_OPTIONAL_CLASS,
  getProviderApiKeyLabel,
  type PersonaDropdownOption,
} from "./agentConfigOptions";
import { buildEditAgentProviderDropdownOptions } from "./agentDialogDropdowns";
import {
  getBakedProviderInheritLabel,
  type InheritedDefault,
} from "./bakedEnvHelpers";
import { PersonaDropdownField } from "./PersonaDropdownField";
import { PersonaProviderApiKeyField } from "./PersonaProviderApiKeyField";

/**
 * LLM provider + provider API key + model block of the Edit Agent dialog.
 *
 * Provider and credential fields remain presentational. The dialog supplies
 * its canonical model/profile section so Hermes ownership and write-through
 * behavior stay on the shared model surface.
 */
export function EditAgentProviderModelFields({
  customCommand,
  disabled,
  llmProviderFieldVisible,
  providerRequired,
  providerDropdownOptions,
  providerSelectValue,
  onProviderDropdownChange,
  isCustomProviderEditing,
  provider,
  onProviderChange,
  topLevelSecretEnvVar,
  apiKeyIsInherited,
  apiKeyInheritedLabel,
  apiKeyIsRequired,
  effectiveProvider,
  apiKeyValue,
  onApiKeyChange,
  modelSection,
}: {
  customCommand?: { value: string; onChange: (value: string) => void };
  disabled: boolean;
  llmProviderFieldVisible: boolean;
  providerRequired: boolean;
  providerDropdownOptions: PersonaDropdownOption[];
  providerSelectValue: string;
  onProviderDropdownChange: (value: string) => void;
  isCustomProviderEditing: boolean;
  provider: string;
  onProviderChange: (value: string) => void;
  topLevelSecretEnvVar: string | null;
  apiKeyIsInherited: boolean;
  apiKeyInheritedLabel: string;
  apiKeyIsRequired: boolean;
  effectiveProvider: string;
  apiKeyValue: string;
  onApiKeyChange: (value: string) => void;
  modelSection: ReactNode;
}) {
  return (
    <>
      {customCommand ? (
        <div className="space-y-1.5">
          <label
            className="text-sm font-medium text-foreground"
            htmlFor="edit-agent-command"
          >
            Agent command
          </label>
          <div
            className={cn(
              "flex min-h-11 items-center px-3",
              PERSONA_FIELD_SHELL_CLASS,
            )}
          >
            <Input
              autoCorrect="off"
              className={cn(
                "h-8 px-0 py-0 leading-6",
                PERSONA_FIELD_CONTROL_CLASS,
              )}
              disabled={disabled}
              id="edit-agent-command"
              onChange={(event) => customCommand.onChange(event.target.value)}
              placeholder="Full path or shell command"
              value={customCommand.value}
            />
          </div>
        </div>
      ) : null}
      {/* LLM provider */}
      {llmProviderFieldVisible ? (
        <div className="space-y-1.5">
          <label
            className="text-sm font-medium text-foreground"
            htmlFor="edit-agent-llm-provider"
          >
            LLM provider
            {providerRequired ? (
              <span className="ml-1 text-destructive" aria-hidden="true">
                *
              </span>
            ) : (
              <span className={PERSONA_LABEL_OPTIONAL_CLASS}>Optional</span>
            )}
          </label>
          <PersonaDropdownField
            disabled={disabled}
            id="edit-agent-llm-provider"
            onValueChange={onProviderDropdownChange}
            options={providerDropdownOptions}
            placeholder="Default (auto)"
            value={providerSelectValue}
          />
          {isCustomProviderEditing ? (
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
                  AUTO_PROVIDER_DROPDOWN_VALUE,
                  BLOCK_BUILD_HIDDEN_PROVIDER_IDS,
                  CUSTOM_PROVIDER_DROPDOWN_VALUE,
                  getPersonaProviderOptions,
                  PERSONA_FIELD_CONTROL_CLASS,
                )}
                disabled={disabled}
                id="edit-agent-custom-provider"
                onChange={(event) => onProviderChange(event.target.value)}
                placeholder="Custom provider ID"
                value={provider}
              />
            </div>
          ) : null}
        </div>
      ) : null}

      {llmProviderFieldVisible && topLevelSecretEnvVar ? (
        <PersonaProviderApiKeyField
          disabled={disabled}
          envVarName={topLevelSecretEnvVar}
          isInherited={apiKeyIsInherited}
          inheritedLabel={apiKeyInheritedLabel}
          isRequired={apiKeyIsRequired}
          label={getProviderApiKeyLabel(effectiveProvider) ?? "API Key"}
          onValueChange={onApiKeyChange}
          value={apiKeyValue}
        />
      ) : null}

      {modelSection}
    </>
  );
}

/** Resolve provider choices using the same inherited defaults as the dialog. */
export function editAgentProviderFields({
  provider,
  runtimeId,
  inheritedProviderDefault,
  isCustomProviderEditing,
  bakedEnvKeys,
}: {
  provider: string;
  runtimeId: string;
  inheritedProviderDefault: InheritedDefault;
  isCustomProviderEditing: boolean;
  bakedEnvKeys: string[] | undefined;
}) {
  const trimmedProvider = provider.trim();
  const providerOptions = getPersonaProviderOptions(
    trimmedProvider,
    runtimeId,
    inheritedProviderDefault.source === "global"
      ? inheritedProviderDefault.value
      : "",
    (bakedEnvKeys ?? []).includes("BUZZ_AGENT_PROVIDER")
      ? BLOCK_BUILD_HIDDEN_PROVIDER_IDS
      : new Set<string>(),
  );
  return {
    providerSelectValue: isCustomProviderEditing
      ? CUSTOM_PROVIDER_DROPDOWN_VALUE
      : trimmedProvider || AUTO_PROVIDER_DROPDOWN_VALUE,
    providerDropdownOptions: buildEditAgentProviderDropdownOptions(
      providerOptions,
      {
        inheritedFromBuild: inheritedProviderDefault.source === "build",
        mapBlankBuildLabel: () =>
          getBakedProviderInheritLabel(
            inheritedProviderDefault.value,
            providerOptions,
          ),
      },
    ),
  };
}
