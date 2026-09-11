import * as React from "react";

import {
  useAvailableAcpRuntimes,
  useHermesProfilesQuery,
} from "@/features/agents/hooks";
import type { OwnerOperationScope } from "@/shared/api/ownerOperations";
import {
  useSetWikiRuntimeSettings,
  useWikiRuntimeSettings,
  type WikiRuntimeSelection,
} from "@/features/wiki/hooks/useWikiRuntimeSettings";

const KNOWN_RUNTIME_LABELS: Record<string, string> = {
  hermes: "Hermes",
  claude: "Claude Code",
  codex: "Codex",
};

const KNOWN_RUNTIME_IDS = ["hermes", "claude", "codex"] as const;

export function WikiRuntimeSettingsControl({
  owner,
  repoD,
  expected,
}: {
  owner?: string;
  repoD?: string;
  expected?: OwnerOperationScope;
}) {
  const coordinate = owner && repoD ? `30617:${owner}:${repoD}` : undefined;
  const [open, setOpen] = React.useState(false);
  const settingsQuery = useWikiRuntimeSettings(coordinate, expected, {
    enabled: open,
  });
  const saveMutation = useSetWikiRuntimeSettings();
  const runtimeQuery = useAvailableAcpRuntimes({
    enabled: Boolean(coordinate) && open,
  });
  const [runtimeId, setRuntimeId] = React.useState("hermes");
  const [profile, setProfile] = React.useState("default");
  const [model, setModel] = React.useState("");

  const availableRuntimes = React.useMemo(() => {
    const entries = (runtimeQuery.data ?? [])
      .filter((runtime) =>
        (KNOWN_RUNTIME_IDS as readonly string[]).includes(runtime.id),
      )
      .map((runtime) => ({ id: runtime.id, label: runtime.label }));
    if (entries.length > 0) return entries;
    return KNOWN_RUNTIME_IDS.map((id) => ({
      id,
      label: KNOWN_RUNTIME_LABELS[id],
    }));
  }, [runtimeQuery.data]);
  const profilesQuery = useHermesProfilesQuery({
    enabled: open && runtimeId === "hermes",
  });

  React.useEffect(() => {
    if (!settingsQuery.isSuccess) return;
    const saved = settingsQuery.data;
    if (saved) {
      setRuntimeId(saved.runtimeId);
      setProfile(saved.profile ?? "default");
      setModel(saved.model ?? "");
      return;
    }
    const first = availableRuntimes[0]?.id ?? "hermes";
    setRuntimeId(first);
    setProfile("default");
    setModel("");
  }, [availableRuntimes, settingsQuery.data, settingsQuery.isSuccess]);

  if (!coordinate || !expected) return null;
  const activeCoordinate = coordinate;
  const activeExpected = expected;

  const savedRuntimeId = settingsQuery.data?.runtimeId;
  const savedRuntimeLabel = savedRuntimeId
    ? (availableRuntimes.find((runtime) => runtime.id === savedRuntimeId)
        ?.label ??
      KNOWN_RUNTIME_LABELS[savedRuntimeId] ??
      savedRuntimeId)
    : null;
  const savedLabel = settingsQuery.data
    ? settingsQuery.data.runtimeId === "hermes"
      ? `${savedRuntimeLabel} / ${settingsQuery.data.profile ?? "unknown"}`
      : `${savedRuntimeLabel} / ${settingsQuery.data.model ?? "unknown"}`
    : "Not configured";
  const invalidDraft =
    runtimeId === "hermes"
      ? profile.trim().length === 0
      : model.trim().length === 0;

  function save() {
    const selection: WikiRuntimeSelection =
      runtimeId === "hermes"
        ? { runtimeId, profile: profile.trim(), model: null }
        : { runtimeId, model: model.trim(), profile: null };
    saveMutation.mutate({
      coordinate: activeCoordinate,
      expected: activeExpected,
      selection,
    });
  }

  return (
    <div className="relative" data-testid="wiki-runtime-settings">
      <button
        aria-expanded={open}
        className="rounded border border-border px-1.5 py-0.5 text-foreground"
        data-testid="wiki-runtime-settings-toggle"
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        Runtime: {savedLabel}
      </button>
      {open ? (
        <div
          className="absolute right-0 z-20 mt-2 w-72 rounded-md border border-border bg-card p-3 text-xs shadow-lg"
          data-testid="wiki-runtime-settings-panel"
        >
          <p className="mb-2 font-medium text-foreground">Wiki runtime</p>
          <p className="mb-3 text-muted-foreground">
            Choose the installed runtime used for this repository. This binding
            is separate from employee agents.
          </p>
          <label className="mb-2 block">
            <span className="mb-1 block text-muted-foreground">Runtime</span>
            <select
              aria-label="Wiki runtime"
              className="w-full rounded border border-input bg-background px-2 py-1 text-foreground"
              data-testid="wiki-runtime-select"
              onChange={(event) => setRuntimeId(event.target.value)}
              value={runtimeId}
            >
              {availableRuntimes.map((runtime) => (
                <option key={runtime.id} value={runtime.id}>
                  {runtime.label}
                </option>
              ))}
            </select>
          </label>
          {runtimeId === "hermes" ? (
            <label className="mb-2 block">
              <span className="mb-1 block text-muted-foreground">
                Hermes profile
              </span>
              <input
                aria-label="Hermes profile"
                className="w-full rounded border border-input bg-background px-2 py-1 text-foreground"
                list="wiki-hermes-profiles"
                onChange={(event) => setProfile(event.target.value)}
                placeholder="default"
                value={profile}
              />
              <datalist id="wiki-hermes-profiles">
                {(profilesQuery.data ?? []).map((name) => (
                  <option key={name} value={name} />
                ))}
              </datalist>
            </label>
          ) : (
            <label className="mb-2 block">
              <span className="mb-1 block text-muted-foreground">Model</span>
              <input
                aria-label="Wiki runtime model"
                className="w-full rounded border border-input bg-background px-2 py-1 text-foreground"
                onChange={(event) => setModel(event.target.value)}
                placeholder="model identifier"
                value={model}
              />
            </label>
          )}
          {settingsQuery.error ? (
            <p className="mb-2 text-destructive" role="alert">
              {settingsQuery.error instanceof Error
                ? settingsQuery.error.message
                : "Wiki runtime settings could not be read."}
            </p>
          ) : null}
          {saveMutation.error ? (
            <p className="mb-2 text-destructive" role="alert">
              {saveMutation.error instanceof Error
                ? saveMutation.error.message
                : "Wiki runtime settings could not be saved."}
            </p>
          ) : null}
          <button
            className="rounded bg-primary px-2 py-1 text-primary-foreground disabled:opacity-50"
            data-testid="wiki-runtime-settings-save"
            disabled={invalidDraft || saveMutation.isPending}
            onClick={save}
            type="button"
          >
            {saveMutation.isPending ? "Saving…" : "Save runtime"}
          </button>
        </div>
      ) : null}
    </div>
  );
}
