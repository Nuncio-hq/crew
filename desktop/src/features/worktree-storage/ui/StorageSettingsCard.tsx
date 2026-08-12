import * as React from "react";

import { formatDiskBytes } from "@/features/channels/lib/worktreeDiskFormat";
import { useProjectsQuery } from "@/features/projects/hooks";
import { SettingsSectionHeader } from "@/features/settings/ui/SettingsSectionHeader";
import {
  formatAbsenceBanner,
  formatObservedIdleLine,
  repositoryLabel,
} from "@/features/worktree-storage/lib/formatObservedIdle";
import { runStorageCleanup } from "@/features/worktree-storage/lib/runStorageCleanup";
import { getWorktreeStorageSnapshot } from "@/shared/api/agentControl";
import type {
  WorktreeStorageRow,
  WorktreeStorageRowOutcome,
  WorktreeStorageSnapshot,
} from "@/shared/api/thread-workspace-types";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";

type LoadState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; snapshot: WorktreeStorageSnapshot }
  | { status: "error"; message: string };

export function StorageSettingsCard() {
  const projectsQuery = useProjectsQuery();
  const [load, setLoad] = React.useState<LoadState>({ status: "idle" });
  const [selected, setSelected] = React.useState<Set<string>>(() => new Set());
  const [outcomes, setOutcomes] = React.useState<
    Map<string, WorktreeStorageRowOutcome>
  >(() => new Map());
  const [running, setRunning] = React.useState(false);

  const repositoryPaths = React.useMemo(() => {
    const paths = new Set<string>();
    for (const project of projectsQuery.data ?? []) {
      for (const repository of project.repositories) {
        if (repository.localWorkspacePath) {
          paths.add(repository.localWorkspacePath);
        }
      }
    }
    return [...paths];
  }, [projectsQuery.data]);

  const refresh = React.useCallback(async () => {
    setLoad({ status: "loading" });
    try {
      const snapshot = await getWorktreeStorageSnapshot(repositoryPaths);
      setLoad({ status: "ready", snapshot });
      const next = new Set<string>();
      for (const row of snapshot.rows) {
        if (row.candidate) next.add(row.worktreePath);
      }
      setSelected(next);
      setOutcomes(new Map());
    } catch (error) {
      setLoad({
        status: "error",
        message:
          error instanceof Error
            ? error.message
            : "Could not load local storage.",
      });
    }
  }, [repositoryPaths]);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  const snapshot = load.status === "ready" ? load.snapshot : null;
  const candidateRows =
    snapshot?.rows.filter(
      (row) => row.candidate && selected.has(row.worktreePath),
    ) ?? [];
  const selectedBytes = candidateRows.reduce((sum, row) => {
    if (row.tier === "hibernate") return sum + row.diskBytes;
    return sum + row.cacheBytes;
  }, 0);

  const runCleanup = async () => {
    if (!snapshot || candidateRows.length === 0 || running) return;
    setRunning(true);
    try {
      await runStorageCleanup({
        rows: candidateRows,
        onProgress: ({ worktreePath, outcome }) => {
          setOutcomes((prev) => {
            const next = new Map(prev);
            next.set(worktreePath, outcome);
            return next;
          });
        },
      });
      // Keep per-row outcomes in place (no summary modal). Owner refreshes
      // explicitly when they want a new measurement pass.
    } finally {
      setRunning(false);
    }
  };

  const toggleRow = (path: string, enabled: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (enabled) next.add(path);
      else next.delete(path);
      return next;
    });
  };

  const absence =
    snapshot != null ? formatAbsenceBanner(snapshot.recentAbsenceSecs) : null;

  return (
    <section className="space-y-4" data-testid="settings-storage">
      <SettingsSectionHeader
        description="See managed worktree disk use and reclaim cache or idle checkouts. Suggest-and-confirm only — no background auto-GC."
        title="Local storage"
      />

      <div
        className="rounded-lg border border-border/60 bg-muted/20 p-4"
        data-testid="storage-header"
      >
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <h3 className="text-base font-medium">Local storage</h3>
          <p
            className="text-sm text-muted-foreground"
            data-testid="storage-total"
          >
            {snapshot
              ? `${formatDiskBytes(snapshot.totalDiskBytes)} total`
              : load.status === "loading"
                ? "Measuring…"
                : "—"}
          </p>
        </div>

        {snapshot && snapshot.candidateCount > 0 ? (
          <div className="mt-3 space-y-1" data-testid="storage-suggestion">
            <p className="text-sm">
              {snapshot.candidateCount} thread
              {snapshot.candidateCount === 1 ? "" : "s"} reclaimable — free up ~
              {formatDiskBytes(snapshot.reclaimableBytes)}
            </p>
            {absence ? (
              <p
                className="text-sm text-muted-foreground"
                data-testid="storage-absence-banner"
              >
                {absence}
              </p>
            ) : null}
          </div>
        ) : snapshot ? (
          <div className="mt-3 space-y-1">
            <p className="text-sm text-muted-foreground">
              No reclaim candidates right now.
            </p>
            {absence ? (
              <p
                className="text-sm text-muted-foreground"
                data-testid="storage-absence-banner"
              >
                {absence}
              </p>
            ) : null}
          </div>
        ) : null}

        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            data-testid="storage-refresh"
            disabled={load.status === "loading" || running}
            onClick={() => void refresh()}
            size="sm"
            variant="outline"
          >
            Refresh
          </Button>
        </div>
      </div>

      {load.status === "error" ? (
        <p className="text-sm text-destructive" data-testid="storage-error">
          {load.message}
        </p>
      ) : null}

      {snapshot ? (
        <ul className="divide-y divide-border/60 rounded-lg border border-border/60">
          {snapshot.rows.length === 0 ? (
            <li className="p-4 text-sm text-muted-foreground">
              No managed worktrees found for linked Project folders.
            </li>
          ) : (
            snapshot.rows.map((row) => (
              <StorageRow
                key={row.worktreePath}
                disabled={running}
                onToggle={toggleRow}
                outcome={outcomes.get(row.worktreePath)}
                row={row}
                selected={selected.has(row.worktreePath)}
              />
            ))
          )}
        </ul>
      ) : null}

      {snapshot?.rows.some((row) => row.candidate) ? (
        <div
          className="flex flex-wrap items-center justify-between gap-3"
          data-testid="storage-footer"
        >
          <p className="text-sm text-muted-foreground">
            Selected: {candidateRows.length} thread
            {candidateRows.length === 1 ? "" : "s"} · ~
            {formatDiskBytes(selectedBytes)}
          </p>
          <Button
            data-testid="storage-run-cleanup"
            disabled={candidateRows.length === 0 || running}
            onClick={() => void runCleanup()}
          >
            {running ? "Running…" : "Run cleanup"}
          </Button>
        </div>
      ) : null}
    </section>
  );
}

function StorageRow({
  row,
  selected,
  disabled,
  outcome,
  onToggle,
}: {
  row: WorktreeStorageRow;
  selected: boolean;
  disabled: boolean;
  outcome: WorktreeStorageRowOutcome | undefined;
  onToggle: (path: string, enabled: boolean) => void;
}) {
  const readOnly = row.readOnly || !row.candidate;
  const outcomeLabel =
    outcome?.status === "completed"
      ? `✓ ${outcome.message}`
      : outcome?.status === "skipped"
        ? `⏭ ${outcome.message}`
        : outcome?.status === "running"
          ? "Running…"
          : null;

  return (
    <li
      className="flex gap-3 p-3"
      data-candidate={row.candidate ? "true" : "false"}
      data-testid={`storage-row-${row.worktreeName}`}
      data-tier={row.tier ?? "none"}
    >
      <div className="pt-0.5">
        {readOnly ? (
          <span aria-hidden className="inline-block w-4" />
        ) : (
          <Checkbox
            checked={selected}
            data-testid={`storage-select-${row.worktreeName}`}
            disabled={disabled || Boolean(outcome)}
            onCheckedChange={(value) =>
              onToggle(row.worktreePath, value === true)
            }
          />
        )}
      </div>
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <p className="truncate text-base font-medium">{row.worktreeName}</p>
          <p className="text-sm text-muted-foreground">
            {row.prNumber != null
              ? `PR #${row.prNumber} ${row.prState ?? ""}`.trim()
              : "No PR"}{" "}
            · {formatDiskBytes(row.cacheBytes)} cache
          </p>
        </div>
        {outcomeLabel ? (
          <p className="text-sm" data-testid="storage-row-outcome">
            {outcomeLabel}
          </p>
        ) : readOnly && row.refusalReason ? (
          <p className="text-sm" data-testid="storage-row-refusal">
            ⛔ {row.refusalReason}
          </p>
        ) : (
          <>
            <p className="text-sm text-muted-foreground">
              {repositoryLabel(row.repositoryPath)}
              {" · "}
              {formatObservedIdleLine({
                observedIdleSecs: row.observedIdleSecs,
                wallIdleSecs: row.wallIdleSecs,
              })}
            </p>
            <p className="text-sm" data-testid="storage-row-reason">
              → {row.reason}
            </p>
          </>
        )}
      </div>
    </li>
  );
}
