import * as React from "react";
import { ChevronDown } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import type {
  AcpRuntimeCatalogEntry,
  CreatePersonaInput,
  UpdatePersonaInput,
} from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { AgentCreationPreview } from "./AgentCreationPreview";
import type { EnvVarsValue } from "./EnvVarsEditor";
import { PersonaAdvancedFields } from "./PersonaAdvancedFields";
import { runtimeAvailabilityWarning } from "./runtimeAvailabilityWarning";
import {
  canSubmitPersonaDialog,
  formatPersonaNamePoolText,
  parsePersonaNamePoolText,
} from "./personaDialogState";
import { hasText } from "./personaDialogEnvVars";
import {
  behaviorForSubmit,
  draftFromBehavior,
  emptyPersonaBehaviorDraft,
  personaBehaviorDraftValid,
} from "./personaBehaviorDraft";
import {
  ADVANCED_FIELDS_MOTION_TRANSITION,
  AUTO_MODEL_DROPDOWN_VALUE,
  AUTO_PROVIDER_DROPDOWN_VALUE,
  BLOCK_BUILD_HIDDEN_PROVIDER_IDS,
  buildPersonaRuntimeDropdownOptions,
  CUSTOM_PROVIDER_DROPDOWN_VALUE,
  computeLocalModeGate,
  formatRuntimeOptionLabel,
  getDefaultPersonaRuntime,
  getPersonaModelOptions,
  getPersonaProviderOptions,
  getProviderApiKeyLabel,
  getRuntimePersonaModelOptions,
  NO_RUNTIME_DROPDOWN_VALUE,
  runtimeSupportsLlmProviderSelection,
  type PersonaDropdownOption,
} from "./agentConfigOptions";
import {
  decorateAutomaticModelOptions,
  modelAfterAutomaticDropdownChange,
  resolveAutomaticModelUiState,
} from "./automaticModelUi";
import { buildProviderDropdownOptions } from "./agentDialogDropdowns";
import { useClearKnownModelOnScopeChange } from "./useClearKnownModelOnScopeChange";
import {
  modelDropdownOptions as buildModelDropdownOptions,
  relayMeshModelPickerState,
} from "./relayMeshModelPicker";
import {
  selectionOnModelDropdownChange,
  selectionOnProviderDropdownChange,
  selectionOnRuntimeChange,
  type RuntimeModelProviderSelection,
} from "./runtimeModelProviderSelection";
import {
  MODEL_DISCOVERY_LOADING_VALUE,
  usePersonaModelDiscovery,
} from "./usePersonaModelDiscovery";
import { useBakedBuildEnvKeysQuery, useRuntimeFileConfigQuery } from "../hooks";
import { useAgentDialogDefaults } from "./useAgentDialogDefaults";
import { AgentDefaultsDialog } from "./AgentDefaultsDialog";
import { AgentHarnessField } from "./AgentHarnessField";
import {
  AgentAiConfigurationModeField,
  AgentCreateAiDefaultsSummary,
  type AgentAiConfigurationMode,
} from "./AgentAiConfigurationMode";
import {
  agentAiConfigurationPairForMode,
  agentAiConfigurationSubmitBlockReason,
  initialAgentAiConfigurationMode,
} from "./agentAiConfigurationPolicy";
import { useProviderApiKeyFieldState } from "./providerApiKeyFieldState";
import { useCreateHermesBinding } from "./createHermesBindingFields";
import { resolveHermesProfileForCreate } from "../lib/hermesProfileBinding";
import { deriveModelFieldVisibility } from "../lib/modelFieldVisibility";
import { AgentDefinitionCustomAiFields } from "./AgentDefinitionCustomAiFields";
import { AgentDefinitionIdentityFields } from "./AgentDefinitionIdentityFields";
import { buildRuntimeModelProviderPayload } from "./agentDefinitionSubmitPayload";
import { AgentDefinitionDialogFooter } from "./AgentDefinitionDialogFooter";
import { AgentDefinitionDialogShell } from "./AgentDefinitionDialogShell";
import { AddCustomHarnessDialog } from "./AddCustomHarnessDialog";
import {
  ADD_CUSTOM_HARNESS_OPTION,
  runtimeDropdownAction,
  usePendingHarnessSelection,
} from "./addCustomHarness";
import { shouldShowStandaloneRuntimeUnavailableWarning } from "./personaSubmitFeedback";

type AgentDefinitionDialogProps = {
  open: boolean;
  embedded?: boolean;
  title: string;
  description: string;
  submitLabel: string;
  initialValues: CreatePersonaInput | UpdatePersonaInput | null;
  error: Error | null;
  isPending: boolean;
  runtimes: AcpRuntimeCatalogEntry[];
  runtimeCatalogStatus?: "loading" | "ready" | "error";
  onOpenChange: (open: boolean) => void;
  onSubmit: (
    input: CreatePersonaInput | UpdatePersonaInput,
    options: AgentDefinitionSubmitOptions,
  ) => Promise<unknown>;
  /** Publishes saved changes when the edited agent is shared in the catalog. */
  publishCatalogUpdatesOnSave?: boolean;
  createRunSection?: React.ReactNode;
  /** Extra create-mode submit gate (e.g. incomplete provider config). */
  createSubmitBlocked?: boolean;
  /** Embedded create: notify the catalog shell about unsaved edits. */
  onDirtyChange?: (dirty: boolean) => void;
};

export type AgentDefinitionSubmitOptions = {
  publishCatalogUpdates: boolean;
  /** Instance binding collected on create-start; ignored on definition edit. */
  hermesProfile?: string | null;
};

export function AgentDefinitionDialog({
  open,
  embedded = false,
  title,
  description,
  submitLabel,
  initialValues,
  error,
  isPending,
  runtimes,
  runtimeCatalogStatus = "ready" as const,
  onOpenChange,
  onSubmit,
  publishCatalogUpdatesOnSave = false,
  createRunSection,
  createSubmitBlocked = false,
  onDirtyChange,
}: AgentDefinitionDialogProps) {
  const runtimesLoading = runtimeCatalogStatus === "loading";
  const [displayName, setDisplayName] = React.useState("");
  const [aiDefaultsOpen, setAiDefaultsOpen] = React.useState(false);
  const aiDefaultsTriggerRef = React.useRef<HTMLButtonElement>(null);
  const [avatarUrl, setAvatarUrl] = React.useState("");
  const [systemPrompt, setSystemPrompt] = React.useState("");
  const [runtime, setRuntime] = React.useState("");
  const [model, setModel] = React.useState("");
  const [hermesProfile, setHermesProfile] = React.useState("");
  const [isCustomModelEditing, setIsCustomModelEditing] = React.useState(false);
  const [provider, setProvider] = React.useState("");
  const [aiConfigurationMode, setAiConfigurationMode] =
    React.useState<AgentAiConfigurationMode>("defaults");
  const [isCustomProviderEditing, setIsCustomProviderEditing] =
    React.useState(false);
  const [namePoolText, setNamePoolText] = React.useState("");
  const [envVars, setEnvVars] = React.useState<EnvVarsValue>({});
  const [behaviorDraft, setBehaviorDraft] = React.useState(
    emptyPersonaBehaviorDraft,
  );
  // The seed the draft is diffed against at submit: an untouched quad
  // submits no behavior group, keeping unrelated edits hash-quiet.
  const behaviorSeedRef = React.useRef(emptyPersonaBehaviorDraft);
  // Tracks when the runtime was auto-seeded by the default-runtime effect in
  // edit mode (i.e. the user never explicitly chose a runtime). Used to omit
  // the seeded runtime from the submit payload for builtin definitions whose
  // canonical runtime is null — the sync would revert it anyway.
  const isRuntimeAutoSeededRef = React.useRef(false);
  // Guards the seeding effect so it fires at most once per dialog-open.
  // Without this, clearing runtime back to "" via "No preference" would re-
  // trigger the effect (the `runtime` dep would pass the length guard) and
  // snap the dropdown back to the default — an edit-mode regression.
  const hasSeededForOpenRef = React.useRef(false);
  const [showAdvancedFields, setShowAdvancedFields] = React.useState(false);
  const [isAvatarUploadPending, setIsAvatarUploadPending] =
    React.useState(false);
  const [hasUserChanges, setHasUserChanges] = React.useState(false);
  const [isAddHarnessOpen, setIsAddHarnessOpen] = React.useState(false);
  React.useEffect(() => {
    onDirtyChange?.(hasUserChanges);
  }, [hasUserChanges, onDirtyChange]);
  const {
    globalConfig,
    inheritedDefaults: {
      provider: inheritedProviderDefault,
      model: inheritedModelDefault,
    },
    inheritedEnvVars: inheritedEnvVarsForAdvanced,
  } = useAgentDialogDefaults({ open });
  const defaultRuntime = React.useMemo(
    () => getDefaultPersonaRuntime(runtimes, globalConfig.preferred_runtime),
    [globalConfig.preferred_runtime, runtimes],
  );
  const isCreateMode = Boolean(initialValues && !("id" in initialValues));
  const shouldReduceMotion = useReducedMotion();
  const initialModelProviderEditableWithoutRuntime = Boolean(
    initialValues &&
      "id" in initialValues &&
      !hasText(initialValues.runtime) &&
      (hasText(initialValues.model) || hasText(initialValues.provider)),
  );

  React.useEffect(() => {
    if (!open || !initialValues) {
      return;
    }

    setDisplayName(initialValues.displayName);
    setAvatarUrl(initialValues.avatarUrl ?? "");
    setSystemPrompt(initialValues.systemPrompt);
    setRuntime(initialValues.runtime ?? "");
    setModel(initialValues.model ?? "");
    setHermesProfile("");
    setIsCustomModelEditing(false);
    setProvider(initialValues.provider ?? "");
    setAiConfigurationMode(
      initialAgentAiConfigurationMode({
        provider: initialValues.provider ?? "",
        model: initialValues.model ?? "",
      }),
    );
    setIsCustomProviderEditing(false);
    const nextNamePoolText =
      "namePool" in initialValues
        ? formatPersonaNamePoolText(initialValues.namePool)
        : "";
    const nextEnvVars =
      "envVars" in initialValues ? (initialValues.envVars ?? {}) : {};
    const nextBehaviorDraft = draftFromBehavior(initialValues.behavior);
    behaviorSeedRef.current = draftFromBehavior(initialValues.behavior);
    setBehaviorDraft(nextBehaviorDraft);
    setNamePoolText(nextNamePoolText);
    setEnvVars(nextEnvVars);
    // Advanced always starts collapsed and only changes from its toggle.
    setShowAdvancedFields(false);
    setIsAvatarUploadPending(false);
    setHasUserChanges(false);
    isRuntimeAutoSeededRef.current = false;
    hasSeededForOpenRef.current = false;
  }, [initialValues, open]);

  React.useEffect(() => {
    if (
      !open ||
      !initialValues ||
      initialValues.runtime?.trim() ||
      runtimesLoading ||
      runtime.trim().length > 0 ||
      defaultRuntime === null ||
      hasSeededForOpenRef.current
    ) {
      return;
    }

    setRuntime(defaultRuntime.id);
    hasSeededForOpenRef.current = true;
    if ("id" in initialValues) {
      // Edit mode: record that this runtime was auto-seeded so the submit path
      // can omit it from the payload for builtin definitions (canonical runtime
      // null; sync would revert the value anyway). Explicit user changes via
      // the dropdown clear this flag.
      isRuntimeAutoSeededRef.current = true;
    }
  }, [defaultRuntime, initialValues, open, runtime, runtimesLoading]);

  // Keep an inherited Create runtime synced with defaults saved in-place.
  React.useEffect(() => {
    if (
      !open ||
      !initialValues ||
      "id" in initialValues ||
      initialValues.runtime?.trim() ||
      aiConfigurationMode !== "defaults" ||
      runtimesLoading ||
      defaultRuntime === null ||
      (runtime.trim().length > 0 && !isRuntimeAutoSeededRef.current)
    ) {
      return;
    }

    if (runtime !== defaultRuntime.id) setRuntime(defaultRuntime.id);
    isRuntimeAutoSeededRef.current = true;
    hasSeededForOpenRef.current = true;
  }, [
    aiConfigurationMode,
    defaultRuntime,
    initialValues,
    open,
    runtime,
    runtimesLoading,
  ]);

  // Keep setup guidance reachable when no available runtime can be inherited.
  React.useEffect(() => {
    if (
      open &&
      isCreateMode &&
      !runtimesLoading &&
      defaultRuntime === null &&
      runtime.trim().length === 0
    ) {
      setAiConfigurationMode("custom");
    }
  }, [defaultRuntime, isCreateMode, open, runtime, runtimesLoading]);

  function handleOpenChange(next: boolean) {
    // Embedded create lives inside the catalog discard flow: Cancel must ask
    // before wiping fields, so defer reset until the parent actually unmounts.
    if (!next && !embedded) {
      setDisplayName("");
      setAvatarUrl("");
      setSystemPrompt("");
      setRuntime("");
      setModel("");
      setHermesProfile("");
      setIsCustomModelEditing(false);
      setProvider("");
      setAiConfigurationMode("defaults");
      setIsCustomProviderEditing(false);
      setNamePoolText("");
      setEnvVars({});
      setBehaviorDraft(emptyPersonaBehaviorDraft);
      behaviorSeedRef.current = emptyPersonaBehaviorDraft;
      setShowAdvancedFields(false);
      setIsAvatarUploadPending(false);
      setHasUserChanges(false);
      setIsAddHarnessOpen(false);
      // isRuntimeAutoSeededRef and hasSeededForOpenRef are NOT reset here — the
      // [initialValues, open] effect resets both when the dialog re-opens.
    }
    onOpenChange(next);
  }
  async function handleSubmit() {
    if (!initialValues || !localModeSatisfied || !canSubmit) return;
    const {
      runtime: runtimeForSubmit,
      model: modelForSubmit,
      provider: providerForSubmit,
    } = buildRuntimeModelProviderPayload({
      runtime,
      model: aiConfigurationMode === "defaults" ? "" : model,
      provider: aiConfigurationMode === "defaults" ? "" : provider,
      isEditMode: "id" in initialValues,
      isAutoSeeded: isRuntimeAutoSeededRef.current,
      initialPreviousRuntime: initialValues.runtime?.trim() ?? "",
      initialModel: initialValues.model,
      initialProvider: initialValues.provider,
      initialModelProviderEditableWithoutRuntime,
      modelWriteThrough,
    });
    const namePool = parsePersonaNamePoolText(namePoolText);
    const namePoolInput =
      namePool.length > 0
        ? namePool
        : "namePool" in initialValues
          ? []
          : undefined;
    const baseInput = {
      displayName: displayName.trim(),
      avatarUrl: avatarUrl.trim() || undefined,
      systemPrompt: systemPrompt,
      runtime: runtimeForSubmit,
      model: modelForSubmit,
      provider: providerForSubmit,
      namePool: namePoolInput,
      envVars,
      behavior: behaviorForSubmit(
        behaviorDraft,
        behaviorSeedRef.current,
        "id" in initialValues,
      ),
    };

    if ("id" in initialValues) {
      await onSubmit(
        {
          id: initialValues.id,
          ...baseInput,
        },
        {
          publishCatalogUpdates: publishCatalogUpdatesOnSave && hasUserChanges,
        },
      );
      return;
    }

    await onSubmit(baseInput, {
      publishCatalogUpdates: false,
      hermesProfile: resolveHermesProfileForCreate(
        hermesProfile,
        selectedRuntime,
      ),
    });
  }

  function handleSubmitForm(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void handleSubmit();
  }

  const selectedRuntime = runtimes.find((p) => p.id === runtime);
  const {
    showProfileField: showHermesProfileField,
    modelOwnedByProfile,
    modelWriteThrough,
    profileError: hermesProfileError,
  } = useCreateHermesBinding({
    enabled: isCreateMode,
    hermesProfile,
    onHermesProfileChange: setHermesProfile,
    respondTo: behaviorDraft.respondTo,
    runtime: selectedRuntime,
  });
  const blankRuntimeModelProviderEditable =
    initialModelProviderEditableWithoutRuntime && runtime.trim().length === 0;
  const runtimeCanChooseLlmProvider =
    runtimeSupportsLlmProviderSelection(runtime) ||
    blankRuntimeModelProviderEditable;
  const llmProviderFieldVisible =
    (runtime.trim().length > 0 && runtimeCanChooseLlmProvider) ||
    blankRuntimeModelProviderEditable;
  const trimmedProvider = provider.trim();
  // Required credential env keys and file-layer config; silences requirements satisfied in the file layer.
  const { data: runtimeFileConfig } = useRuntimeFileConfigQuery(runtime, {
    enabled: open,
  });
  function handleAiConfigurationModeChange(nextMode: AgentAiConfigurationMode) {
    setHasUserChanges(true);
    setAiConfigurationMode(nextMode);
    setIsCustomProviderEditing(false);
    setIsCustomModelEditing(false);
    const nextPair = agentAiConfigurationPairForMode({
      current: { provider, model },
      inherited: runtimeCanChooseLlmProvider
        ? {
            provider: inheritedProviderDefault.value,
            model: inheritedModelDefault.value,
          }
        : { provider: "", model: runtimeFileConfig?.model?.trim() ?? "" },
      mode: nextMode,
      needsProviderSelection: runtimeCanChooseLlmProvider,
    });
    setProvider(nextPair.provider);
    setModel(nextPair.model);
  }
  const { data: bakedEnvKeys } = useBakedBuildEnvKeysQuery({ enabled: open });
  const localModeGate = React.useMemo(
    () =>
      computeLocalModeGate({
        bakedEnvKeys,
        envVars,
        globalEnvVars: globalConfig.env_vars,
        globalProvider: inheritedProviderDefault.value,
        globalModel: inheritedModelDefault.value,
        isProviderMode: false,
        model,
        provider: trimmedProvider,
        runtimeId: runtime,
        runtimeFileConfig,
      }),
    [
      bakedEnvKeys,
      envVars,
      globalConfig.env_vars,
      inheritedModelDefault.value,
      inheritedProviderDefault.value,
      model,
      trimmedProvider,
      runtime,
      runtimeFileConfig,
    ],
  );
  // requiredEnvKeys: the gate already handles baked-, global-, and file-
  // satisfied keys so no further filtering is needed.
  const { requiredEnvKeys } = localModeGate;
  const localModeSatisfied = localModeGate.satisfied;
  // Effective provider: agent value → global fallback → file fallback.
  // Mirrors the chain inside computeLocalModeGate so model-option scoping and
  // model requiredness are consistent with the readiness gate.
  const fileProvider = runtimeFileConfig?.provider?.trim() ?? "";
  const effectiveProvider =
    trimmedProvider || inheritedProviderDefault.value || fileProvider;
  const apiKeyFieldState = useProviderApiKeyFieldState({
    bakedEnvKeys,
    effectiveEnvVars: envVars,
    envVars,
    fileSatisfiedEnvKeys: localModeGate.fileSatisfiedEnvKeys,
    globalEnvVars: globalConfig.env_vars,
    provider: effectiveProvider,
    requiredEnvKeys,
  });
  const {
    advancedRequiredEnvKeys,
    inheritedLabel: apiKeyInheritedLabel,
    isInherited: apiKeyIsInherited,
    isRequired: apiKeyIsRequired,
    secretEnvVar: topLevelSecretEnvVar,
    value: apiKeyValue,
  } = apiKeyFieldState;
  const { modelFieldVisible, providerIsRequired } = deriveModelFieldVisibility({
    aiConfigurationMode,
    blankRuntimeModelProviderEditable,
    modelOwnedByProfile,
    modelWriteThrough,
    runtime,
    runtimeCanChooseLlmProvider,
  });
  const isExplicitModelRequired =
    aiConfigurationMode === "custom" && modelFieldVisible;
  const customAiPairBlockReason = agentAiConfigurationSubmitBlockReason(
    aiConfigurationMode,
    { provider, model },
    runtimeCanChooseLlmProvider,
    modelFieldVisible,
  );
  const customAiPairSatisfied = customAiPairBlockReason == null;
  const selectedRuntimeIsAvailable =
    runtime.trim().length === 0 ||
    selectedRuntime?.availability === "available";
  // Gate model/provider validity through missingNormalizedFields — single
  // source of truth with the readiness gate so display and Save can't drift.
  const canSubmit =
    canSubmitPersonaDialog({ displayName, isPending }) &&
    (!isCreateMode || runtime.trim().length > 0) &&
    (!isCreateMode || selectedRuntimeIsAvailable) &&
    (!isCreateMode || !createSubmitBlocked) &&
    // Crash-loop guard, create AND edit: an empty allowlist would crash
    // every instance minted from this definition at startup.
    personaBehaviorDraftValid(behaviorDraft) &&
    // D1: localModeSatisfied covers both missingNormalizedFields AND
    // missingEnvKeys — credential env keys now block submit, not just display.
    localModeSatisfied &&
    customAiPairSatisfied &&
    hermesProfileError == null &&
    !isAvatarUploadPending;
  // Merge global env as the base layer so credential keys satisfied via global
  // config are available to model discovery — same rationale as in AgentInstanceEditDialog.
  const envVarsForDiscovery = React.useMemo(
    () => ({ ...globalConfig.env_vars, ...envVars }),
    [globalConfig.env_vars, envVars],
  );
  const {
    discoveredModelOptions,
    modelDiscoveryLoading,
    modelDiscoveryStatus,
  } = usePersonaModelDiscovery({
    envVars: envVarsForDiscovery,
    isCustomProviderEditing,
    modelFieldVisible,
    open,
    // Gate provider by runtime: runtimes that don't support LLM provider
    // selection (codex, claude) must not inherit the global provider — doing
    // so causes them to discover models from the wrong provider.
    provider: runtimeSupportsLlmProviderSelection(runtime)
      ? effectiveProvider
      : "",
    selectedRuntime,
  });
  const staticModelOptions = getPersonaModelOptions(runtime, effectiveProvider);
  const runtimeModelOptions = getRuntimePersonaModelOptions(runtime);
  const {
    isCustom: isModelCustom,
    isRelayMesh,
    options: modelOptions,
    selectValue: modelSelectValue,
    showCustomInput: showCustomModelInput,
  } = relayMeshModelPickerState({
    discoveredOptions: discoveredModelOptions,
    fallbackOptions: staticModelOptions,
    knownOptions: discoveredModelOptions ?? runtimeModelOptions,
    isCustomEditing: isCustomModelEditing,
    model,
    modelFieldVisible,
    provider: effectiveProvider,
  });
  const { offerAutomaticModel, resolvedModelSelectValue, selectableAutoModel } =
    resolveAutomaticModelUiState({
      isRelayMesh,
      model,
      modelSelectValue,
      runtime: selectedRuntime ?? { id: runtime, modelEnvVar: null },
    });
  // On internal Block builds, BUZZ_AGENT_PROVIDER is baked in and a boot
  // migration rewrites any persisted Databricks v1 values → v2. Hide the v1
  // option there so it is not offered for new selections. OSS builds have no
  // baked provider, so v1 remains visible.
  const hideProviderIds = React.useMemo(
    () =>
      (bakedEnvKeys ?? []).includes("BUZZ_AGENT_PROVIDER")
        ? BLOCK_BUILD_HIDDEN_PROVIDER_IDS
        : new Set<string>(),
    [bakedEnvKeys],
  );
  const providerOptions = getPersonaProviderOptions(
    trimmedProvider,
    runtime,
    inheritedProviderDefault.source === "global"
      ? inheritedProviderDefault.value
      : "",
    hideProviderIds,
  );
  const providerSelectValue = isCustomProviderEditing
    ? CUSTOM_PROVIDER_DROPDOWN_VALUE
    : trimmedProvider || AUTO_PROVIDER_DROPDOWN_VALUE;
  const showCustomProviderInput =
    llmProviderFieldVisible && isCustomProviderEditing;
  const runtimeDropdownValue = runtime.trim() || NO_RUNTIME_DROPDOWN_VALUE;
  const { blankRuntimeOptionLabel, runtimeDropdownOptions } =
    buildPersonaRuntimeDropdownOptions({
      defaultRuntimeId: defaultRuntime?.id,
      isCreateMode,
      runtime,
      runtimes,
      runtimesLoading,
    });
  runtimeDropdownOptions.push(ADD_CUSTOM_HARNESS_OPTION);
  const runtimeSummaryLabel = selectedRuntime
    ? formatRuntimeOptionLabel(selectedRuntime)
    : runtime.trim() || "Not configured";
  const providerDropdownOptions: PersonaDropdownOption[] =
    buildProviderDropdownOptions(providerOptions);
  const modelDropdownOptions: PersonaDropdownOption[] =
    decorateAutomaticModelOptions(
      buildModelDropdownOptions({
        allowCustom: !isRelayMesh,
        globalModel: undefined,
        loading: modelDiscoveryLoading && discoveredModelOptions === null,
        loadingValue: MODEL_DISCOVERY_LOADING_VALUE,
        options: modelOptions,
      }),
      { isRelayMesh, offerAutomaticModel, selectableAutoModel },
    );
  const previewLabel = displayName.trim() || "Agent name";
  const previewAvatarUrl = avatarUrl.trim() || null;
  const runtimeWarningText = selectedRuntime
    ? runtimeAvailabilityWarning(selectedRuntime)
    : null;
  const runtimeWarning = runtimeWarningText ? (
    <p className="text-xs text-warning">
      {runtimeWarningText} Visit Settings &gt; Agents to set it up.
    </p>
  ) : null;
  // Harness picker (and its inline warning) only render in Customize mode.
  // Profile-owned runtimes (Hermes) hide AI-configuration tabs entirely when
  // preferred, so create stays on defaults with no harness field — still show
  // the unavailable-runtime blocker so Add agent is not silently disabled.
  const showStandaloneRuntimeWarning =
    shouldShowStandaloneRuntimeUnavailableWarning({
      isCreateMode,
      hasRuntimeWarning: Boolean(runtimeWarningText),
      aiConfigurationMode,
    });
  const advancedFieldsTransition = shouldReduceMotion
    ? { duration: 0 }
    : ADVANCED_FIELDS_MOTION_TRANSITION;
  useClearKnownModelOnScopeChange({
    open,
    modelFieldVisible,
    isCustomModelEditing,
    model,
    provider: effectiveProvider,
    runtime,
    setModel,
    setIsCustomModelEditing,
  });

  const selection: RuntimeModelProviderSelection = {
    provider,
    model,
    isCustomProviderEditing,
    isCustomModelEditing,
    envVars,
  };

  function applySelection(next: RuntimeModelProviderSelection) {
    setProvider(next.provider);
    setModel(next.model);
    setIsCustomProviderEditing(next.isCustomProviderEditing);
    setIsCustomModelEditing(next.isCustomModelEditing);
    setEnvVars(next.envVars);
  }

  function handleRuntimeDropdownChange(nextValue: string) {
    const action = runtimeDropdownAction(nextValue);
    if (action.kind === "add-custom-harness") {
      setIsAddHarnessOpen(true);
      return;
    }
    setHasUserChanges(true);
    const nextRuntime = action.runtimeId;
    // The user made an explicit choice — no longer auto-seeded.
    isRuntimeAutoSeededRef.current = false;
    setRuntime(nextRuntime);
    applySelection(
      selectionOnRuntimeChange(selection, {
        previousRuntime: runtime,
        nextRuntime,
        nextRuntimeCanChooseProvider:
          nextRuntime.trim().length > 0 &&
          runtimeSupportsLlmProviderSelection(nextRuntime),
        lockedRuntimeReset: "full",
      }),
    );
  }

  // Routed through the normal change handler so a harness registered inline
  // resets model/provider exactly as a hand-picked one would. Scoped to `open`
  // so a pending id can't outlive the dialog that started the registration.
  const selectSavedHarness = usePendingHarnessSelection(
    runtimes,
    handleRuntimeDropdownChange,
    open,
  );

  function handleProviderDropdownChange(nextValue: string) {
    setHasUserChanges(true);
    const nextProvider =
      nextValue === AUTO_PROVIDER_DROPDOWN_VALUE ? "" : nextValue;
    if (nextProvider === "relay-mesh" && runtime !== "buzz-agent") {
      handleRuntimeDropdownChange("buzz-agent");
    }
    const nextSelection = selectionOnProviderDropdownChange(selection, {
      runtime: nextProvider === "relay-mesh" ? "buzz-agent" : runtime,
      nextValue,
      clearModelWhenApiKeyMissing: true,
    });
    applySelection({
      ...nextSelection,
      model: nextProvider === "relay-mesh" ? "auto" : nextSelection.model,
    });
  }

  function handleModelDropdownChange(nextValue: string) {
    setHasUserChanges(true);
    const nextSelection = selectionOnModelDropdownChange(selection, {
      nextValue,
      clearKnownModelOnCustomEntry: true,
      isModelCustom,
    });
    applySelection({
      ...nextSelection,
      model: modelAfterAutomaticDropdownChange({
        isRelayMesh,
        nextSelectionModel: nextSelection.model,
        nextValue,
        selectableAutoModel,
      }),
    });
  }

  const footer = (
    <AgentDefinitionDialogFooter
      canSubmit={canSubmit}
      isAvatarUploadPending={isAvatarUploadPending}
      isPending={isPending}
      onCancel={() => handleOpenChange(false)}
      publishesCatalogUpdates={publishCatalogUpdatesOnSave && hasUserChanges}
      submitBlockReason={customAiPairBlockReason}
      submitLabel={submitLabel}
    />
  );

  const form = (
    <form
      className="grid gap-5 lg:grid-cols-[220px_minmax(0,1fr)]"
      id="persona-dialog-form"
      onChangeCapture={() => setHasUserChanges(true)}
      onSubmit={handleSubmitForm}
    >
      <AgentCreationPreview
        avatarUrl={previewAvatarUrl}
        disabled={isPending || isAvatarUploadPending}
        label={previewLabel}
        onClearAvatar={() => {
          setHasUserChanges(true);
          setAvatarUrl("");
        }}
        onUploadPendingChange={setIsAvatarUploadPending}
        onSelectAvatar={(nextAvatarUrl) => {
          setHasUserChanges(true);
          setAvatarUrl(nextAvatarUrl);
        }}
        runtimeId={runtime.trim() || null}
      />
      <div className="space-y-5">
        <AgentDefinitionIdentityFields
          displayName={displayName}
          isCreateMode={isCreateMode}
          isPending={isPending}
          onDisplayNameChange={setDisplayName}
          onSystemPromptChange={setSystemPrompt}
          runtimeWarningText={runtimeWarningText}
          showStandaloneRuntimeWarning={showStandaloneRuntimeWarning}
          systemPrompt={systemPrompt}
        />

        {modelFieldVisible ? (
          <AgentAiConfigurationModeField
            mode={aiConfigurationMode}
            needsProviderSelection={runtimeCanChooseLlmProvider}
            onModeChange={handleAiConfigurationModeChange}
          />
        ) : null}

        <div
          className="space-y-5"
          data-testid={`agent-${aiConfigurationMode}-configuration-section`}
        >
          {aiConfigurationMode === "custom" ? (
            <AgentHarnessField
              disabled={isPending || runtimesLoading}
              onValueChange={handleRuntimeDropdownChange}
              options={runtimeDropdownOptions}
              placeholder={blankRuntimeOptionLabel}
              value={runtimeDropdownValue}
              warning={runtimeWarning}
            />
          ) : null}
          <AgentDefinitionCustomAiFields
            aiConfigurationMode={aiConfigurationMode}
            apiKeyInheritedLabel={apiKeyInheritedLabel}
            apiKeyIsInherited={apiKeyIsInherited}
            apiKeyIsRequired={apiKeyIsRequired}
            apiKeyValue={apiKeyValue}
            autoModelDropdownValue={AUTO_MODEL_DROPDOWN_VALUE}
            currentAgentName={displayName}
            disabled={isPending}
            effectiveProviderApiKeyLabel={
              getProviderApiKeyLabel(effectiveProvider) ?? "API key"
            }
            hermesProfile={hermesProfile}
            isExplicitModelRequired={isExplicitModelRequired}
            isRelayMesh={isRelayMesh}
            llmProviderFieldVisible={llmProviderFieldVisible}
            model={model}
            modelDiscoveryStatus={modelDiscoveryStatus}
            modelDropdownOptions={modelDropdownOptions}
            modelFieldVisible={modelFieldVisible}
            modelOwnedByProfile={modelOwnedByProfile}
            modelWriteThrough={modelWriteThrough}
            hasPersona={selectedRuntime?.capabilities?.personaDoc === "soulMd"}
            modelSelectValue={resolvedModelSelectValue}
            onCustomModelChange={setModel}
            onHermesProfileChange={(next) => {
              setHasUserChanges(true);
              setHermesProfile(next);
            }}
            onModelValueChange={handleModelDropdownChange}
            onProviderChange={setProvider}
            onProviderValueChange={handleProviderDropdownChange}
            onSecretEnvChange={(next) => {
              if (!topLevelSecretEnvVar) return;
              setEnvVars((prev) => ({
                ...prev,
                [topLevelSecretEnvVar]: next,
              }));
            }}
            provider={provider}
            providerDropdownOptions={providerDropdownOptions}
            providerIsRequired={providerIsRequired}
            providerSelectValue={providerSelectValue}
            showCustomModelInput={showCustomModelInput}
            showCustomProviderInput={showCustomProviderInput}
            showHermesProfileField={showHermesProfileField}
            respondTo={behaviorDraft.respondTo}
            topLevelSecretEnvVar={topLevelSecretEnvVar}
            transition={advancedFieldsTransition}
          />
          {aiConfigurationMode === "defaults" && !modelWriteThrough ? (
            <AgentCreateAiDefaultsSummary
              canChooseProvider={runtimeCanChooseLlmProvider}
              harness={runtimeSummaryLabel}
              inheritedModel={inheritedModelDefault}
              inheritedProvider={inheritedProviderDefault}
              isConfigured={localModeGate.satisfied}
              model={runtimeFileConfig?.model}
              onEditDefaults={() => setAiDefaultsOpen(true)}
              triggerRef={aiDefaultsTriggerRef}
            />
          ) : null}
        </div>

        <AgentDefaultsDialog
          onOpenChange={setAiDefaultsOpen}
          open={runtimeCanChooseLlmProvider && aiDefaultsOpen}
          returnFocusRef={aiDefaultsTriggerRef}
        />

        <AddCustomHarnessDialog
          onOpenChange={setIsAddHarnessOpen}
          onSaved={selectSavedHarness}
          open={isAddHarnessOpen}
        />
        <div className="space-y-3">
          <button
            aria-expanded={showAdvancedFields}
            className="inline-flex h-9 items-center gap-1.5 text-sm font-medium text-foreground transition-colors hover:text-foreground/80 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            onClick={() => setShowAdvancedFields((current) => !current)}
            type="button"
          >
            <span>Advanced</span>
            {(isCreateMode && createSubmitBlocked) ||
            localModeGate.missingEnvKeys.some((key) =>
              advancedRequiredEnvKeys.includes(key),
            ) ? (
              <span
                aria-hidden="true"
                className="rounded-full bg-destructive/10 px-2 py-0.5 text-xs text-destructive"
                data-testid="persona-advanced-required-badge"
              >
                Required
              </span>
            ) : null}
            <ChevronDown
              className={cn(
                "h-4 w-4 text-muted-foreground transition-transform duration-150 ease-out",
                showAdvancedFields && "rotate-180",
              )}
            />
          </button>
          <AnimatePresence initial={false}>
            {showAdvancedFields ? (
              <motion.div
                animate={{ height: "auto", opacity: 1, scale: 1 }}
                className="origin-top overflow-hidden"
                exit={{ height: 0, opacity: 0, scale: 0.98 }}
                initial={{ height: 0, opacity: 0, scale: 0.98 }}
                key="persona-advanced-fields"
                transition={advancedFieldsTransition}
              >
                <PersonaAdvancedFields
                  afterRespondTo={isCreateMode ? createRunSection : undefined}
                  behaviorDraft={behaviorDraft}
                  disabled={isPending}
                  envVars={envVars}
                  fileSatisfiedEnvKeys={localModeGate.fileSatisfiedEnvKeys}
                  hiddenEnvKeys={
                    topLevelSecretEnvVar ? [topLevelSecretEnvVar] : []
                  }
                  inheritedEnvVars={inheritedEnvVarsForAdvanced}
                  model={model}
                  modelTuningRuntimeId={runtime}
                  namePoolText={namePoolText}
                  catalogStatus={runtimeCatalogStatus}
                  selectedRuntime={selectedRuntime}
                  onBehaviorDraftChange={(nextBehaviorDraft) => {
                    setHasUserChanges(true);
                    setBehaviorDraft(nextBehaviorDraft);
                  }}
                  onEnvVarsChange={setEnvVars}
                  onNamePoolTextChange={setNamePoolText}
                  provider={effectiveProvider}
                  requiredEnvKeys={advancedRequiredEnvKeys}
                />
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
        {error ? (
          <p className="text-sm text-destructive">{error.message}</p>
        ) : null}
      </div>
    </form>
  );

  return (
    <AgentDefinitionDialogShell
      description={description}
      embedded={embedded}
      footer={footer}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && (isPending || isAvatarUploadPending)) return;
        handleOpenChange(nextOpen);
      }}
      open={open}
      title={title}
    >
      {form}
    </AgentDefinitionDialogShell>
  );
}
