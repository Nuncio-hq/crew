import * as React from "react";

import { recapClient as defaultRecapClient, type RecapClient } from "../api";
import {
  defaultRecapSettings,
  hasValidRecapSelection,
  recapRuntimeLabel,
  selectedRuntime,
  type RecapSettings,
  type RecapSettingsSnapshot,
} from "../types";
import { recapErrorMessage } from "../lib/recapState";
import {
  SettingsOptionGroup,
  SettingsOptionGroupList,
  SettingsOptionRow,
} from "@/features/settings/ui/SettingsOptionGroup";
import { SettingsSectionHeader } from "@/features/settings/ui/SettingsSectionHeader";
import { Input } from "@/shared/ui/input";

export type RecapSettingsCardProps = {
  client?: RecapClient;
};

export type RecapSettingsLoadState = "loading" | "unavailable" | "ready";

export function recapSettingsLoadState(
  snapshot: RecapSettingsSnapshot | null,
  error: string | null,
): RecapSettingsLoadState {
  if (snapshot) return "ready";
  return error ? "unavailable" : "loading";
}

export function canSaveRecapSettings(
  draft: RecapSettings,
  snapshot: RecapSettingsSnapshot,
): boolean {
  return hasValidRecapSelection(draft, snapshot.runtimes);
}

function settingValue(settings: RecapSettings): string {
  return JSON.stringify(settings);
}

export function RecapSettingsCard({
  client = defaultRecapClient,
}: RecapSettingsCardProps) {
  const [snapshot, setSnapshot] = React.useState<RecapSettingsSnapshot | null>(
    null,
  );
  const [draft, setDraft] = React.useState<RecapSettings>(defaultRecapSettings);
  const [error, setError] = React.useState<string | null>(null);
  const [status, setStatus] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const loadSequence = React.useRef(0);

  const load = React.useCallback(() => {
    const sequence = loadSequence.current + 1;
    loadSequence.current = sequence;
    let active = true;
    setError(null);
    setStatus(null);
    void client
      .getSettings()
      .then((loaded) => {
        if (!active || sequence !== loadSequence.current) return;
        setSnapshot(loaded);
        setDraft(loaded.settings);
      })
      .catch((loadError: unknown) => {
        if (!active || sequence !== loadSequence.current) return;
        setSnapshot(null);
        setError(recapErrorMessage(loadError));
      });
    return () => {
      active = false;
    };
  }, [client]);

  React.useEffect(() => load(), [load]);

  const loadState = recapSettingsLoadState(snapshot, error);
  if (loadState === "loading") {
    return (
      <section className="min-w-0" data-testid="settings-recap">
        <SettingsSectionHeader
          title="Thread recap"
          description="Choose a certified runtime for a manual, owner-local thread recap."
        />
        <SettingsOptionGroup title="Availability">
          <SettingsOptionRow>
            <p className="text-sm text-muted-foreground" role="status">
              Checking recap runtime support…
            </p>
          </SettingsOptionRow>
        </SettingsOptionGroup>
      </section>
    );
  }

  if (loadState === "unavailable" || !snapshot) {
    return (
      <section className="min-w-0" data-testid="settings-recap">
        <SettingsSectionHeader
          title="Thread recap"
          description="Choose a certified runtime for a manual, owner-local thread recap."
        />
        <SettingsOptionGroup title="Availability">
          <SettingsOptionRow>
            <div className="min-w-0 flex-1 space-y-1">
              <p
                className="text-sm font-medium"
                data-testid="recap-unavailable"
              >
                Off — unavailable
              </p>
              <p
                className="text-sm text-muted-foreground/70"
                data-settings-subcopy
              >
                A certified recap runtime is not available on this installation.
                No agent session or model has been changed.
              </p>
              {error ? (
                <p
                  className="text-xs text-muted-foreground/70"
                  data-testid="recap-settings-error"
                >
                  {error}
                </p>
              ) : null}
            </div>
            <button
              className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-muted"
              data-testid="recap-settings-retry"
              onClick={() => {
                setSnapshot(null);
                setDraft(defaultRecapSettings());
                void load();
              }}
              type="button"
            >
              Retry
            </button>
          </SettingsOptionRow>
        </SettingsOptionGroup>
      </section>
    );
  }

  const runtimes = snapshot.runtimes;
  const runtime = selectedRuntime(draft, runtimes);
  const isSelectionValid = canSaveRecapSettings(draft, snapshot);
  const isDirty = settingValue(draft) !== settingValue(snapshot.settings);
  const supportsProfiles = runtime?.kind === "hermes";
  const profileRequired = supportsProfiles;
  const modelRequired =
    runtime?.kind !== "hermes" && (runtime?.models.length ?? 0) > 0;
  const selectedProfile = runtime?.profiles.find(
    (profile) => profile.id === draft.profileRef,
  );
  const selectionMessage =
    draft.mode === "off"
      ? "Recap is disabled."
      : !runtime
        ? "Choose a runtime to enable manual generation."
        : runtime.availability !== "supported"
          ? runtime.reason || "This runtime is unavailable."
          : profileRequired && !selectedProfile
            ? "Choose a profile before saving."
            : modelRequired && !draft.requestedModel?.trim()
              ? "Choose a model before saving."
              : "Manual generation only; this does not change an agent session.";

  function updateRuntime(runtimeId: string) {
    if (!runtimeId) {
      setDraft((current) => ({
        ...current,
        mode: "off",
        runtimeId: null,
        requestedModel: null,
        profileRef: null,
        capabilityFingerprint: null,
      }));
      setStatus(null);
      return;
    }
    const nextRuntime = runtimes.find((option) => option.id === runtimeId);
    setDraft((current) => ({
      ...current,
      mode: "manual",
      runtimeId,
      requestedModel:
        current.runtimeId === runtimeId ? current.requestedModel : null,
      profileRef: current.runtimeId === runtimeId ? current.profileRef : null,
      capabilityFingerprint: nextRuntime?.capabilityFingerprint ?? null,
    }));
    setStatus(null);
  }

  async function save() {
    if (!isSelectionValid || saving) return;
    setSaving(true);
    setError(null);
    setStatus(null);
    try {
      const saved = await client.saveSettings({
        ...draft,
        requestedModel: draft.requestedModel?.trim() || null,
      });
      setSnapshot(saved);
      setDraft(saved.settings);
      setStatus("Recap settings saved.");
    } catch (saveError: unknown) {
      setError(recapErrorMessage(saveError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="min-w-0" data-testid="settings-recap">
      <SettingsSectionHeader
        title="Thread recap"
        description="Choose a certified runtime for a manual, owner-local thread recap."
      />
      <SettingsOptionGroupList>
        <SettingsOptionGroup
          title="Runtime"
          description="Recaps are generated only when you click Generate in a thread."
        >
          <SettingsOptionRow className="items-start">
            <div className="min-w-0 flex-1 space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="recap-runtime-select"
              >
                Recap runtime
              </label>
              <select
                className="h-9 w-full rounded-md border border-input bg-background px-2 text-sm"
                data-testid="recap-runtime-select"
                disabled={saving}
                id="recap-runtime-select"
                onChange={(event) => updateRuntime(event.target.value)}
                value={draft.runtimeId ?? ""}
              >
                <option value="">Off — no generated recap</option>
                {runtimes.map((option) => (
                  <option
                    disabled={option.availability === "unsupported"}
                    key={option.id}
                    value={option.id}
                  >
                    {option.label}
                    {option.availability === "unsupported"
                      ? " — unavailable"
                      : ""}
                  </option>
                ))}
              </select>
              <p
                className="text-xs text-muted-foreground/70"
                data-settings-subcopy
              >
                {selectionMessage}
              </p>
            </div>
          </SettingsOptionRow>

          {runtime?.kind === "hermes" ? (
            <SettingsOptionRow className="items-start">
              <div className="min-w-0 flex-1 space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="recap-profile-select"
                >
                  Hermes profile
                </label>
                <select
                  className="h-9 w-full rounded-md border border-input bg-background px-2 text-sm"
                  data-testid="recap-profile-select"
                  disabled={saving || runtime.availability !== "supported"}
                  id="recap-profile-select"
                  onChange={(event) => {
                    setDraft((current) => ({
                      ...current,
                      profileRef: event.target.value || null,
                    }));
                    setStatus(null);
                  }}
                  value={draft.profileRef ?? ""}
                >
                  <option value="">Choose a profile</option>
                  {runtime.profiles.map((profile) => (
                    <option key={profile.id} value={profile.id}>
                      {profile.label}
                    </option>
                  ))}
                </select>
                <p
                  className="text-xs text-muted-foreground/70"
                  data-settings-subcopy
                >
                  The selected profile supplies its own model and provider.
                </p>
              </div>
            </SettingsOptionRow>
          ) : null}

          {runtime && runtime.kind !== "hermes" ? (
            <SettingsOptionRow className="items-start">
              <div className="min-w-0 flex-1 space-y-1.5">
                <label
                  className="text-sm font-medium"
                  htmlFor="recap-model-input"
                >
                  Recap model{modelRequired ? " (required)" : ""}
                </label>
                <Input
                  data-testid="recap-model-input"
                  disabled={saving || runtime.availability !== "supported"}
                  id="recap-model-input"
                  onChange={(event) => {
                    setDraft((current) => ({
                      ...current,
                      requestedModel: event.target.value,
                    }));
                    setStatus(null);
                  }}
                  placeholder={
                    modelRequired ? "Choose a model" : "Runtime default"
                  }
                  value={draft.requestedModel ?? ""}
                />
                <p
                  className="text-xs text-muted-foreground/70"
                  data-settings-subcopy
                >
                  {modelRequired
                    ? "Select the certified model for this runtime."
                    : "Optional requested model."}{" "}
                  Effective model provenance is shown on each recap.
                </p>
              </div>
            </SettingsOptionRow>
          ) : null}

          <SettingsOptionRow>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium">When to recap</p>
              <p
                className="text-sm text-muted-foreground/70"
                data-settings-subcopy
              >
                Only when I click Generate recap
              </p>
            </div>
            <span className="text-xs text-muted-foreground">Manual</span>
          </SettingsOptionRow>

          <SettingsOptionRow className="items-start">
            <div className="min-w-0 flex-1 space-y-1">
              <p
                className="text-sm text-muted-foreground/70"
                data-settings-subcopy
              >
                Recaps are owner-local reading aids. They do not post to a
                channel, change an employee model, or establish a plan or
                accepted result.
              </p>
              {runtime ? (
                <p className="text-xs text-muted-foreground/70">
                  Selected: {recapRuntimeLabel(draft, runtimes)}
                </p>
              ) : null}
              {error ? (
                <p className="text-xs text-destructive" role="alert">
                  {error}
                </p>
              ) : null}
              {status ? (
                <p className="text-xs text-muted-foreground" role="status">
                  {status}
                </p>
              ) : null}
            </div>
            <div className="flex shrink-0 gap-2">
              <button
                className="rounded-md border border-border px-3 py-1.5 text-sm hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                data-testid="recap-settings-cancel"
                disabled={!isDirty || saving}
                onClick={() => {
                  setDraft(snapshot.settings);
                  setError(null);
                  setStatus(null);
                }}
                type="button"
              >
                Cancel
              </button>
              <button
                className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
                data-testid="recap-settings-save"
                disabled={!isDirty || !isSelectionValid || saving}
                onClick={() => void save()}
                type="button"
              >
                {saving ? "Saving…" : "Save settings"}
              </button>
            </div>
          </SettingsOptionRow>
        </SettingsOptionGroup>
      </SettingsOptionGroupList>
    </section>
  );
}
