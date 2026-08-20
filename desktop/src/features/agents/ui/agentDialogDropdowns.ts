import type { PersonaDropdownOption } from "./agentConfigOptions";
import {
  AUTO_PROVIDER_DROPDOWN_VALUE,
  CUSTOM_PROVIDER_DROPDOWN_VALUE,
} from "./agentConfigOptions";

type ProviderOption = { id: string; label: string };

/** Shared provider `<Select>` options for agent create/edit dialogs. */
export function buildProviderDropdownOptions(
  providerOptions: ReadonlyArray<ProviderOption>,
  {
    includeBlankIds = false,
    mapLabel = (option: ProviderOption) => option.label,
    mapValue = (option: ProviderOption) => option.id,
  }: {
    includeBlankIds?: boolean;
    mapLabel?: (option: ProviderOption) => string;
    mapValue?: (option: ProviderOption) => string;
  } = {},
): PersonaDropdownOption[] {
  const visible = includeBlankIds
    ? providerOptions
    : providerOptions.filter((option) => option.id.trim().length > 0);
  return [
    ...visible.map((option) => ({
      label: mapLabel(option),
      value: mapValue(option),
    })),
    { label: "Custom provider...", value: CUSTOM_PROVIDER_DROPDOWN_VALUE },
  ];
}

export function buildEditAgentProviderDropdownOptions(
  providerOptions: ReadonlyArray<ProviderOption>,
  {
    inheritedFromBuild,
    mapBlankBuildLabel,
  }: {
    inheritedFromBuild: boolean;
    mapBlankBuildLabel: () => string;
  },
): PersonaDropdownOption[] {
  return buildProviderDropdownOptions(providerOptions, {
    includeBlankIds: true,
    mapLabel: (option) =>
      option.id === "" && inheritedFromBuild
        ? mapBlankBuildLabel()
        : option.label,
    mapValue: (option) => option.id || AUTO_PROVIDER_DROPDOWN_VALUE,
  });
}
