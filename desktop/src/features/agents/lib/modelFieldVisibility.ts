export function modelFieldVisibility({
  modelOwnedByProfile,
  modelWriteThrough,
  runtime,
  blankRuntimeModelProviderEditable,
}: {
  modelOwnedByProfile: boolean;
  modelWriteThrough: boolean;
  runtime: string;
  blankRuntimeModelProviderEditable: boolean;
}) {
  return (
    !modelOwnedByProfile &&
    !modelWriteThrough &&
    (runtime.trim().length > 0 || blankRuntimeModelProviderEditable)
  );
}

export function providerRequirementVisible({
  aiConfigurationMode,
  runtimeCanChooseLlmProvider,
}: {
  aiConfigurationMode: "defaults" | "custom";
  runtimeCanChooseLlmProvider: boolean;
}) {
  // Gate the provider requirement on the field's actual visibility, not the raw
  // runtime capability. Codex/Claude hide the provider picker (they drive their
  // own provider), so Customize must not require a provider there. But a
  // runtime-less legacy/builtin definition still exposes the picker via
  // blankRuntimeModelProviderEditable, so it must keep requiring a provider —
  // otherwise Save could persist `provider: undefined` despite the visible field.
  return aiConfigurationMode === "custom" && runtimeCanChooseLlmProvider;
}

export function deriveModelFieldVisibility({
  aiConfigurationMode,
  blankRuntimeModelProviderEditable,
  modelOwnedByProfile,
  modelWriteThrough,
  runtime,
  runtimeCanChooseLlmProvider,
}: {
  aiConfigurationMode: "defaults" | "custom";
  blankRuntimeModelProviderEditable: boolean;
  modelOwnedByProfile: boolean;
  modelWriteThrough: boolean;
  runtime: string;
  runtimeCanChooseLlmProvider: boolean;
}) {
  return {
    modelFieldVisible: modelFieldVisibility({
      blankRuntimeModelProviderEditable,
      modelOwnedByProfile,
      modelWriteThrough,
      runtime,
    }),
    providerIsRequired: providerRequirementVisible({
      aiConfigurationMode,
      runtimeCanChooseLlmProvider,
    }),
  };
}
