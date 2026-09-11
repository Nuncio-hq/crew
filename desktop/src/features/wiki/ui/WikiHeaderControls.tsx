import {
  useWikiGenerate,
  useWikiSetCadence,
} from "@/features/wiki/hooks/useWikiGenerate";
import {
  repoKey,
  wikiCanCancelRecovery,
  wikiFreshness,
  wikiRecoveryActionLabel,
  wikiRecoveryAffordance,
  type WikiCadence,
  type WikiJobState,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import type { OwnerOperationScope } from "@/shared/api/ownerOperations";
import type { RelayEvent } from "@/shared/api/types";
import { WikiRuntimeSettingsControl } from "@/features/wiki/ui/WikiRuntimeSettingsControl";

export function WikiHeaderControls({
  toc,
  owner,
  repoD,
  repoPath,
  workspaceMode,
  onOpenProject,
  onSearchChange,
  operationScope,
  search,
  showCadence,
  repoState,
  recoveryJob,
  onRecoveryRetry,
  onRecoveryReconcile,
  onRecoveryCancel,
  onRegenerate,
  recoveryPending,
  regeneratePending,
}: {
  toc: WikiToc | null;
  owner?: string;
  repoD?: string;
  repoPath?: string | null;
  workspaceMode?: "git" | "folder";
  onOpenProject?: () => void;
  onSearchChange?: (value: string) => void;
  operationScope?: OwnerOperationScope;
  search?: string;
  showCadence: boolean;
  repoState?: RelayEvent;
  recoveryJob?: WikiJobState;
  onRecoveryRetry?: () => void;
  onRecoveryReconcile?: () => void;
  onRecoveryCancel?: () => void;
  onRegenerate?: () => void;
  recoveryPending?: boolean;
  regeneratePending?: boolean;
}) {
  const setCadence = useWikiSetCadence();
  const generate = useWikiGenerate();
  const cadence = toc?.cadence ?? "manual";
  const freshness = wikiFreshness(toc, repoState);
  const canEditOwner = Boolean(
    owner &&
      operationScope &&
      operationScope.scope.owner.toLowerCase() === owner.toLowerCase(),
  );
  const canGenerate = Boolean(owner && repoD && canEditOwner);
  const cadenceError = setCadence.error
    ? setCadence.error instanceof Error
      ? setCadence.error.message
      : String(setCadence.error)
    : null;
  const generateError = generate.error
    ? generate.error instanceof Error
      ? generate.error.message
      : String(generate.error)
    : null;
  const affordance = wikiRecoveryAffordance(recoveryJob);
  // Retry and Resume are the same native explicit action; a retired snapshot
  // must never be offered either of them.
  const publicationAction =
    affordance === "retry" || affordance === "resume"
      ? wikiRecoveryActionLabel(affordance)
      : null;
  return (
    <div className="flex items-center gap-2 text-2xs text-muted-foreground">
      <span
        className={freshness === "stale" ? "text-attention" : undefined}
        data-testid="wiki-freshness"
      >
        {freshness === "stale"
          ? `Stale · Last updated ${toc ? formatAge(toc.generatedAt) : ""}`
          : freshness === "unknown"
            ? "Freshness unavailable"
            : toc
              ? `Last updated ${formatAge(toc.generatedAt)}`
              : "Never generated"}
      </span>
      <span>⑂ {toc?.branch || "main"}</span>
      {canGenerate ? (
        <WikiRuntimeSettingsControl
          expected={operationScope}
          owner={owner}
          repoD={repoD}
        />
      ) : null}
      {showCadence && canEditOwner ? (
        <label className="flex items-center gap-1">
          Auto:
          <select
            aria-label="Wiki cadence"
            className="rounded border border-border bg-background"
            data-testid="wiki-cadence"
            disabled={setCadence.isPending}
            onChange={(event) => {
              if (
                setCadence.isPending ||
                !toc ||
                !owner ||
                !repoD ||
                !operationScope
              )
                return;
              setCadence.mutate({
                owner,
                repoD,
                cadence: event.target.value,
                repoKey: repoKey(owner, repoD),
                expectedScope: operationScope,
              });
            }}
            value={cadence}
          >
            {(["manual", "on-push", "daily", "weekly"] as WikiCadence[]).map(
              (value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ),
            )}
          </select>
        </label>
      ) : null}
      {canGenerate ? (
        <button
          className="rounded-md border border-input bg-card px-2 py-0.5 text-foreground"
          data-testid="wiki-generate-mirror"
          disabled={generate.isPending || setCadence.isPending}
          onClick={() => {
            if (!owner || !repoD || !operationScope) return;
            generate.mutate({
              owner,
              repoD,
              repoKey: repoKey(owner, repoD),
              repoPath,
              workspaceMode,
              expectedScope: operationScope,
            });
          }}
          type="button"
        >
          {freshness === "stale" ? "Regenerate" : "Generate"}
        </button>
      ) : null}
      {cadenceError ? (
        <span
          className="text-destructive"
          data-testid="wiki-cadence-error"
          role="alert"
        >
          {cadenceError}
        </span>
      ) : null}
      {generateError ? (
        <span
          className="text-destructive"
          data-testid="wiki-generate-error"
          role="alert"
        >
          {generateError}
        </span>
      ) : null}
      {recoveryJob?.operationId ? (
        <span
          className="flex items-center gap-1"
          data-testid="wiki-recovery-header"
        >
          <span className="text-muted-foreground">
            Recovery: {recoveryJob.nativeStatus ?? "pending"}
          </span>
          {affordance === "regenerate" ? (
            <span className="text-attention">
              Immutable snapshot retired; regenerate from source.
            </span>
          ) : null}
          {affordance === "resume" ? (
            <span className="text-attention">
              Canceled with an unresolved attempt; resume or reconcile it.
            </span>
          ) : null}
          {onRecoveryRetry && publicationAction ? (
            <button
              className="rounded border border-border px-1.5 py-0.5 text-foreground"
              data-testid="wiki-recovery-publication-action"
              disabled={recoveryPending}
              onClick={onRecoveryRetry}
              type="button"
            >
              {publicationAction}
            </button>
          ) : null}
          {onRecoveryReconcile && affordance !== "none" ? (
            <button
              className="rounded border border-border px-1.5 py-0.5 text-attention"
              disabled={recoveryPending}
              onClick={onRecoveryReconcile}
              type="button"
            >
              Reconcile
            </button>
          ) : null}
          {onRecoveryCancel && wikiCanCancelRecovery(recoveryJob) ? (
            <button
              className="rounded border border-border px-1.5 py-0.5 text-destructive"
              disabled={recoveryPending}
              onClick={onRecoveryCancel}
              type="button"
            >
              Cancel
            </button>
          ) : null}
          {onRegenerate && affordance === "regenerate" ? (
            <button
              className="rounded border border-attention px-1.5 py-0.5 text-attention"
              data-testid="wiki-regenerate-recovery"
              disabled={regeneratePending || recoveryPending}
              onClick={onRegenerate}
              type="button"
            >
              Regenerate from source
            </button>
          ) : null}
        </span>
      ) : null}
      {onSearchChange ? (
        <input
          aria-label="Search wiki"
          className="h-7 w-32 rounded-md border border-border bg-background px-2 text-2xs"
          data-testid="wiki-page-search"
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder="Search"
          value={search ?? ""}
        />
      ) : null}
      {onOpenProject ? (
        <button className="underline" onClick={onOpenProject} type="button">
          Open project
        </button>
      ) : null}
    </div>
  );
}

function formatAge(unix: number): string {
  const delta = Math.max(0, Date.now() / 1000 - unix);
  if (delta < 86400)
    return `${Math.max(1, Math.round(delta / 3600))} hours ago`;
  return `${Math.round(delta / 86400)} days ago`;
}
