import * as React from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/shared/ui/dialog";

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
  generation,
}: {
  owner?: string;
  repoD?: string;
  expected?: OwnerOperationScope;
  generation?: {
    repoName: string;
    repoPath?: string | null;
    branch?: string | null;
    hasPages: boolean;
    pending?: boolean;
    testId?: string;
    onStart: () => void;
  };
}) {
  const coordinate = owner && repoD ? `30617:${owner}:${repoD}` : undefined;
  const [open, setOpen] = React.useState(false);
  const settingsQuery = useWikiRuntimeSettings(coordinate, expected);
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
    return entries;
  }, [runtimeQuery.data]);
  const profilesQuery = useHermesProfilesQuery({
    enabled: open && runtimeId === "hermes",
  });

  const draftInitialized = React.useRef(false);
  const activeGeneration = React.useRef(0);
  const profilesId = React.useId();
  // biome-ignore lint/correctness/useExhaustiveDependencies: Scope changes invalidate an open draft and pending save completion.
  React.useEffect(() => {
    activeGeneration.current += 1;
    draftInitialized.current = false;
    setOpen(false);
    return () => {
      activeGeneration.current += 1;
    };
  }, [
    coordinate,
    expected?.scope.owner,
    expected?.scope.community,
    expected?.workspace_generation,
    expected?.identity_generation,
  ]);

  React.useEffect(() => {
    if (
      !open ||
      draftInitialized.current ||
      !settingsQuery.isSuccess ||
      !runtimeQuery.isSuccess
    )
      return;
    draftInitialized.current = true;
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
  }, [
    open,
    availableRuntimes,
    settingsQuery.data,
    settingsQuery.isSuccess,
    runtimeQuery.isSuccess,
  ]);

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
    : settingsQuery.isPending
      ? "Loading…"
      : settingsQuery.isError
        ? "Unavailable"
        : "Not configured";
  const invalidDraft =
    runtimeId === "hermes"
      ? profile.trim().length === 0
      : model.trim().length === 0;

  async function save() {
    const started = activeGeneration.current;
    const selection: WikiRuntimeSelection =
      runtimeId === "hermes"
        ? { runtimeId, profile: profile.trim(), model: null }
        : { runtimeId, model: model.trim(), profile: null };
    try {
      await saveMutation.mutateAsync({
        coordinate: activeCoordinate,
        expected: activeExpected,
        selection,
      });
      if (started !== activeGeneration.current) return;
      setOpen(false);
      generation?.onStart();
    } catch {
      // The mutation exposes its error in the dialog; never start after a failed save.
    }
  }

  function changeOpen(next: boolean) {
    if (saveMutation.isPending) return;
    draftInitialized.current = false;
    saveMutation.reset();
    setOpen(next);
  }

  return (
    <div className="relative" data-testid="wiki-runtime-settings">
      {generation ? (
        <span
          className="mr-2 text-xs text-muted-foreground"
          data-testid="wiki-runtime-label"
        >
          Runtime: {savedLabel}
        </span>
      ) : null}
      <Dialog open={open} onOpenChange={changeOpen}>
        <DialogTrigger asChild>
          <button
            className="rounded-md border border-input bg-card px-2 py-1 text-foreground disabled:opacity-50"
            data-testid={generation?.testId ?? "wiki-runtime-settings-toggle"}
            disabled={generation?.pending}
            type="button"
          >
            {generation
              ? generation.hasPages
                ? "Update Wiki"
                : "Generate Wiki"
              : `Runtime: ${savedLabel}`}
          </button>
        </DialogTrigger>
        <DialogContent
          className="max-w-lg"
          data-testid="wiki-runtime-settings-panel"
          onEscapeKeyDown={(event) => {
            if (saveMutation.isPending) event.preventDefault();
          }}
          onPointerDownOutside={(event) => {
            if (saveMutation.isPending) event.preventDefault();
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {generation
                ? generation.hasPages
                  ? "Update Wiki"
                  : "Generate Wiki"
                : "Wiki runtime"}
            </DialogTitle>
            <DialogDescription>
              Read a workspace snapshot and publish a checked set of pages.
            </DialogDescription>
          </DialogHeader>
          {generation ? (
            <div className="rounded-lg border border-border bg-muted/30 p-3 text-sm">
              <p className="font-medium">{generation.repoName}</p>
              <p className="break-all text-xs text-muted-foreground">
                {generation.repoPath ?? "No workspace linked"}
              </p>
              {generation.branch ? (
                <p className="mt-1 text-xs text-muted-foreground">
                  {generation.branch}
                </p>
              ) : null}
            </div>
          ) : null}
          <label className="mb-2 block">
            <span className="mb-1 block text-muted-foreground">Runtime</span>
            <select
              aria-label="Wiki runtime"
              className="w-full rounded border border-input bg-background px-2 py-1 text-foreground"
              data-testid="wiki-runtime-select"
              onChange={(event) => setRuntimeId(event.target.value)}
              disabled={
                !runtimeQuery.isSuccess ||
                !settingsQuery.isSuccess ||
                saveMutation.isPending
              }
              value={runtimeId}
            >
              {!availableRuntimes.some(
                (runtime) => runtime.id === runtimeId,
              ) ? (
                <option value={runtimeId} disabled>
                  {KNOWN_RUNTIME_LABELS[runtimeId] ?? runtimeId} (unavailable)
                </option>
              ) : null}
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
                list={profilesId}
                onChange={(event) => setProfile(event.target.value)}
                placeholder="default"
                value={profile}
              />
              <datalist id={profilesId}>
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
          <p className="text-xs text-muted-foreground">
            Runs in a temporary session, separate from Thread Recap and employee
            agents.
          </p>
          {generation ? (
            <>
              <ol className="list-decimal space-y-2 pl-5 text-sm text-muted-foreground">
                <li>Read one workspace snapshot.</li>
                <li>Plan the table of contents.</li>
                <li>Generate changed pages.</li>
                <li>Check source references and publish.</li>
              </ol>
              <p className="text-xs text-muted-foreground">
                Current pages stay readable while the update runs.
              </p>
            </>
          ) : null}
          {runtimeQuery.isError ||
          (runtimeQuery.isSuccess && availableRuntimes.length === 0) ? (
            <p className="text-sm text-destructive" role="alert">
              {runtimeQuery.isError
                ? "Installed runtimes could not be read."
                : "No supported installed runtime is available."}
            </p>
          ) : null}
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
          <DialogFooter>
            <button
              type="button"
              className="rounded border border-input px-3 py-2"
              disabled={saveMutation.isPending}
              onClick={() => changeOpen(false)}
            >
              Cancel
            </button>
            <button
              className="rounded bg-primary px-2 py-1 text-primary-foreground disabled:opacity-50"
              data-testid="wiki-runtime-settings-save"
              disabled={
                invalidDraft ||
                saveMutation.isPending ||
                !settingsQuery.isSuccess ||
                !runtimeQuery.isSuccess ||
                !availableRuntimes.some(
                  (runtime) => runtime.id === runtimeId,
                ) ||
                Boolean(
                  generation && (!generation.repoPath || generation.pending),
                )
              }
              onClick={save}
              type="button"
            >
              {saveMutation.isPending
                ? "Saving…"
                : generation
                  ? generation.hasPages
                    ? "Start update"
                    : "Start generation"
                  : "Save runtime"}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
