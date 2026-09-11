/** The only recap mode exposed in v1: a user-triggered, local reading aid. */
export type RecapMode = "off" | "manual";

/** States returned by the recap service for an existing thread artifact. */
export type RecapStatus =
  | "off"
  | "unsupported"
  | "missing_selection"
  | "no_recap"
  | "generating"
  | "current"
  | "stale"
  | "failed"
  | "cancelled";

export type RecapRuntimeKind = "hermes" | "cli" | "unknown";
export type RecapRuntimeAvailability = "supported" | "unsupported";

export type RecapBounds = {
  maxInputBytes: number;
  maxOutputBytes: number;
  maxWallTimeMs: number;
  cleanupGraceMs: number;
};

export type RecapProfileOption = {
  id: string;
  label: string;
};

export type RecapRuntimeOption = {
  id: string;
  label: string;
  kind: RecapRuntimeKind;
  availability: RecapRuntimeAvailability;
  reason: string | null;
  capabilityFingerprint: string | null;
  profiles: RecapProfileOption[];
  models: string[];
};

export type RecapSettings = {
  version: 1;
  mode: RecapMode;
  runtimeId: string | null;
  requestedModel: string | null;
  profileRef: string | null;
  capabilityFingerprint: string | null;
  bounds: RecapBounds;
};

/** Settings plus the backend-discovered runtime inventory used by Settings. */
export type RecapSettingsSnapshot = {
  settings: RecapSettings;
  runtimes: RecapRuntimeOption[];
};

export type ThreadRecapRequest = {
  channelId: string;
  rootEventId: string;
};

export type ThreadRecapGenerationRequest = ThreadRecapRequest & {
  generationId: string;
};

export type ThreadRecap = {
  generationId: string;
  text: string;
  generatedAt: number;
  runtimeId: string;
  requestedModel: string | null;
  effectiveModel: string | null;
  profileRef: string | null;
  provenance: "verified" | "requested";
  sourceManifestHash: string;
  sourceEventIds: string[];
  omittedMessageCount: number;
  oldestIncludedEventId: string | null;
  newestIncludedEventId: string | null;
};

export type ThreadRecapLookup = {
  status: "no_recap" | "current" | "stale";
  recap: ThreadRecap | null;
  reason?: string | null;
};

export const DEFAULT_RECAP_BOUNDS: RecapBounds = {
  maxInputBytes: 128 * 1024,
  maxOutputBytes: 256 * 1024,
  maxWallTimeMs: 120_000,
  cleanupGraceMs: 5_000,
};

export function defaultRecapSettings(): RecapSettings {
  return {
    version: 1,
    mode: "off",
    runtimeId: null,
    requestedModel: null,
    profileRef: null,
    capabilityFingerprint: null,
    bounds: { ...DEFAULT_RECAP_BOUNDS },
  };
}

export function recapRuntimeLabel(
  settings: Pick<RecapSettings, "runtimeId" | "requestedModel" | "profileRef">,
  runtimes: readonly RecapRuntimeOption[],
): string {
  const runtime = settings.runtimeId
    ? runtimes.find((option) => option.id === settings.runtimeId)
    : undefined;
  const runtimeLabel = runtime?.label ?? settings.runtimeId ?? "Runtime";
  const profile = settings.profileRef
    ? runtime?.profiles.find((option) => option.id === settings.profileRef)
    : undefined;
  if (profile) return `${runtimeLabel} · ${profile.label}`;
  return `${runtimeLabel} · ${settings.requestedModel || "Runtime default"}`;
}

export function selectedRuntime(
  settings: RecapSettings,
  runtimes: readonly RecapRuntimeOption[],
): RecapRuntimeOption | undefined {
  return settings.runtimeId
    ? runtimes.find((option) => option.id === settings.runtimeId)
    : undefined;
}

export function hasValidRecapSelection(
  settings: RecapSettings,
  runtimes: readonly RecapRuntimeOption[],
): boolean {
  if (settings.mode === "off") return true;
  const runtime = selectedRuntime(settings, runtimes);
  if (runtime?.availability !== "supported") return false;
  if (
    runtime.capabilityFingerprint &&
    settings.capabilityFingerprint !== runtime.capabilityFingerprint
  ) {
    return false;
  }
  if (runtime.kind === "hermes") {
    return Boolean(
      settings.profileRef &&
        runtime.profiles.some((profile) => profile.id === settings.profileRef),
    );
  }
  if (settings.profileRef) return false;
  if (runtime.models.length > 0) {
    return Boolean(
      settings.requestedModel?.trim() &&
        runtime.models.includes(settings.requestedModel.trim()),
    );
  }
  return true;
}
