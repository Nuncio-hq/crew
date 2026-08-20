import type { AcpRuntimeCatalogEntry } from "@/shared/api/types";
import { deriveRuntimeCapabilities } from "@/shared/api/runtimeCapabilities";
import {
  AUTO_MODEL_DROPDOWN_VALUE,
  automaticModelOptionLabel,
  automaticModelPersistValue,
  isAutomaticModelSelection,
  shouldOfferAutomaticModelOption,
} from "./agentConfigOptions";

export type AutomaticModelUiState = {
  selectableAutoModel: boolean;
  offerAutomaticModel: boolean;
  resolvedModelSelectValue: string;
};

/** Shared Automatic/Auto row state for agent create/edit dialogs. */
export function resolveAutomaticModelUiState({
  isRelayMesh,
  model,
  modelSelectValue,
  runtime,
}: {
  isRelayMesh: boolean;
  model: string;
  modelSelectValue: string;
  runtime:
    | Pick<AcpRuntimeCatalogEntry, "id" | "modelEnvVar">
    | null
    | undefined;
}): AutomaticModelUiState {
  const selectableAutoModel = deriveRuntimeCapabilities(
    runtime ?? { id: "", modelEnvVar: null },
  ).selectableAutoModel;
  return {
    selectableAutoModel,
    offerAutomaticModel: shouldOfferAutomaticModelOption({
      isRelayMesh,
      selectableAutoModel,
    }),
    resolvedModelSelectValue: isAutomaticModelSelection({
      isRelayMesh,
      model,
      selectableAutoModel,
    })
      ? AUTO_MODEL_DROPDOWN_VALUE
      : modelSelectValue,
  };
}

export function decorateAutomaticModelOptions<
  T extends { value: string; label: string },
>(
  options: T[],
  {
    isRelayMesh,
    offerAutomaticModel,
    selectableAutoModel,
  }: {
    isRelayMesh: boolean;
    offerAutomaticModel: boolean;
    selectableAutoModel: boolean;
  },
): T[] {
  return options
    .filter(
      (option) =>
        offerAutomaticModel || option.value !== AUTO_MODEL_DROPDOWN_VALUE,
    )
    .map((option) =>
      option.value === AUTO_MODEL_DROPDOWN_VALUE
        ? {
            ...option,
            label: automaticModelOptionLabel({
              isRelayMesh,
              selectableAutoModel,
            }),
          }
        : option,
    );
}

/** Persist `"auto"` when the Automatic/Auto row is chosen. */
export function modelAfterAutomaticDropdownChange({
  isRelayMesh,
  nextSelectionModel,
  nextValue,
  selectableAutoModel,
}: {
  isRelayMesh: boolean;
  nextSelectionModel: string;
  nextValue: string;
  selectableAutoModel: boolean;
}): string {
  return nextValue === AUTO_MODEL_DROPDOWN_VALUE
    ? automaticModelPersistValue({ isRelayMesh, selectableAutoModel })
    : nextSelectionModel;
}
