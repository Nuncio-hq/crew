import { truncatePubkey } from "@/shared/lib/pubkey";
import { setTerminalPanelMode } from "@/features/terminal/terminalPanelStore";
import {
  wikiCanCancelRecovery,
  isFailedWikiGeneration,
  isCanceledWikiGeneration,
  wikiGenerationStatusLabel,
  wikiRecoveryActionLabel,
  wikiRecoveryAffordance,
  type WikiFreshness,
  type WikiJobState,
} from "@/features/wiki/lib/wikiEvents";
import type { WikiRepositoryReadStatus } from "@/shared/api/wikiSnapshot";
import type { OwnerOperationScope } from "@/shared/api/ownerOperations";
import {
  WIKI_EMPTY_REPO_COPY,
  type WikiRepoProbe,
} from "@/features/wiki/lib/wikiRepoProbe";
import { WikiRuntimeSettingsControl } from "@/features/wiki/ui/WikiRuntimeSettingsControl";

export function WikiRepoCard({
  name,
  owner,
  repoD,
  repoPath,
  branch,
  description,
  freshness,
  generating,
  updatedAt,
  probe,
  readStatus,
  onOpen,
  onManageWorkspace,
  onGenerate,
  onRetry,
  onRecoveryRetry,
  onRecoveryReconcile,
  onRecoveryCancel,
  onRegenerate,
  operationScope,
  recoveryPending,
  regeneratePending,
}: {
  name: string;
  owner: string;
  repoD: string;
  repoPath?: string | null;
  branch?: string | null;
  description?: string;
  freshness: WikiFreshness | "generating" | "failed";
  generating?: WikiJobState;
  updatedAt: number | null;
  probe?: WikiRepoProbe;
  readStatus?: WikiRepositoryReadStatus;
  onOpen: () => void;
  /** Open the containing Project's workspace management surface. */
  onManageWorkspace?: () => void;
  onGenerate?: () => void;
  onRetry?: () => void;
  onRecoveryRetry?: () => void;
  onRecoveryReconcile?: () => void;
  onRecoveryCancel?: () => void;
  /** Explicit successor action, available only for native retirement proof. */
  onRegenerate?: () => void;
  operationScope?: OwnerOperationScope;
  recoveryPending?: boolean;
  regeneratePending?: boolean;
}) {
  const emptyRepo = probe?.kind === "empty-tree";
  const missingLocalCopy =
    probe?.kind === "missing-local" || probe?.kind === "missing-local-gone"
      ? probe.copy
      : null;
  const readUnavailable = readStatus?.unavailable ?? false;
  const readStale = readStatus?.stale ?? false;
  const durable = Boolean(generating?.operationId);
  const superseded =
    generating?.reconciled && generating.nativeStatus === "superseded";
  const affordance = wikiRecoveryAffordance(generating);
  // Retry and Resume are the same native explicit action with different
  // meaning; a retired snapshot is offered neither.
  const publicationAction =
    affordance === "retry" || affordance === "resume"
      ? wikiRecoveryActionLabel(affordance)
      : null;
  const canReconcile = affordance !== "none" && Boolean(onRecoveryReconcile);
  const canRegenerate = affordance === "regenerate" && Boolean(onRegenerate);
  const canceledGeneration = isCanceledWikiGeneration(generating);
  const failedGeneration = isFailedWikiGeneration(generating);
  // The failure banner offers the durable publication action when there is
  // one. A durable row whose only way forward is Regenerate must not offer a
  // plain "Retry" here: the unique native claim blocks a fresh Generate, and
  // the recovery controls below carry the accurate action.
  const failedAction =
    onRecoveryRetry && publicationAction
      ? { label: publicationAction, run: onRecoveryRetry }
      : null;
  return (
    <div
      className="rounded-xl border border-border bg-card p-4"
      data-testid={`wiki-repo-card-${name}`}
    >
      <button className="w-full text-left" onClick={onOpen} type="button">
        <div className="text-sm font-medium">{name}</div>
        {description ? (
          <p className="mt-2 line-clamp-3 text-sm text-muted-foreground">
            {description}
          </p>
        ) : (
          <div className="text-2xs text-muted-foreground">
            {truncatePubkey(owner)}
          </div>
        )}
      </button>
      <div className="mt-3">
        {readStale ? (
          <p
            className="mb-2 text-2xs text-attention"
            data-testid="wiki-read-stale"
          >
            Showing the last verified Wiki.{" "}
            {readStatus?.message ?? "Refresh unavailable."}
          </p>
        ) : null}
        {readUnavailable ? (
          <div
            className="mb-2 rounded-md bg-destructive/10 p-2 text-2xs text-destructive"
            data-testid="wiki-read-unavailable"
          >
            <p>{readStatus?.message ?? "Wiki read unavailable."}</p>
            {onRetry ? (
              <button
                className="mt-2 rounded-md bg-destructive/15 px-2 py-1"
                data-testid={`wiki-retry-read-${name}`}
                onClick={onRetry}
                type="button"
              >
                Retry read
              </button>
            ) : null}
          </div>
        ) : null}
        {emptyRepo ? (
          <p
            className="text-2xs text-muted-foreground"
            data-testid="wiki-empty-repo"
          >
            {WIKI_EMPTY_REPO_COPY}
          </p>
        ) : null}
        {missingLocalCopy ? (
          <div data-testid="wiki-missing-local">
            <p className="text-2xs text-muted-foreground">{missingLocalCopy}</p>
            {onManageWorkspace ? (
              <button
                className="mt-2 rounded-md border border-input bg-card px-2 py-1 text-2xs text-foreground"
                data-testid={`wiki-manage-workspace-${name}`}
                onClick={onManageWorkspace}
                type="button"
              >
                Manage workspace
              </button>
            ) : null}
          </div>
        ) : null}
        {!emptyRepo && onGenerate && operationScope ? (
          <div className="mb-2">
            <WikiRuntimeSettingsControl
              expected={operationScope}
              owner={owner}
              repoD={repoD}
              generation={{
                repoName: name,
                repoPath,
                branch,
                hasPages: updatedAt !== null,
                pending:
                  freshness === "generating" ||
                  (durable && !generating?.reconciled),
                testId:
                  freshness === "stale"
                    ? `wiki-regenerate-${name}`
                    : `wiki-generate-${name}`,
                onStart: onGenerate,
              }}
            />
          </div>
        ) : null}
        {freshness === "generating" ? (
          <div
            className="text-2xs text-muted-foreground"
            data-testid="wiki-generating"
          >
            {affordance === "regenerate"
              ? "The previous immutable snapshot is retired. Choose Regenerate from source to create a new snapshot."
              : affordance === "resume"
                ? "Publication was canceled with an unresolved attempt. Resume publication to submit the saved snapshot, or Reconcile to check it."
                : wikiGenerationStatusLabel(generating)}
            {generating?.costNote ? (
              <div className="mt-1">{generating.costNote}</div>
            ) : null}
          </div>
        ) : null}
        {canceledGeneration ? (
          <p
            className="text-2xs text-attention"
            data-testid="wiki-generation-canceled"
            role="status"
          >
            {generating?.error ??
              "Wiki generation was canceled before publication. Generate again from source."}
          </p>
        ) : null}
        {failedGeneration ? (
          <p
            className="text-2xs text-destructive"
            data-testid="wiki-generation-failed"
            role="alert"
          >
            {generating?.error ??
              "Wiki generation failed before publication. Generate again from source."}
          </p>
        ) : null}
        {freshness === "fresh" && updatedAt ? (
          <div className="text-2xs text-muted-foreground">
            ⏱ {formatAge(updatedAt)}
          </div>
        ) : null}
        {freshness === "unknown" ? (
          <div
            className="text-2xs text-muted-foreground"
            data-testid="wiki-freshness-unavailable"
          >
            Freshness unavailable
          </div>
        ) : null}
        {freshness === "failed" ? (
          <div data-testid="wiki-failed">
            <p className="mb-1 text-2xs text-destructive">
              {superseded
                ? "Another publication became current. Generate again if this repository still needs this snapshot."
                : (generating?.error ?? "Generation failed")}
            </p>
            <div className="flex gap-2">
              <button
                className="rounded-md bg-destructive/15 px-2 py-1 text-2xs text-destructive"
                onClick={() => setTerminalPanelMode("docked")}
                type="button"
              >
                logs → Term
              </button>
              {failedAction ? (
                <button
                  className="rounded-md bg-destructive/15 px-2 py-1 text-2xs text-destructive"
                  disabled={recoveryPending}
                  onClick={failedAction.run}
                  type="button"
                >
                  {failedAction.label}
                </button>
              ) : null}
            </div>
          </div>
        ) : null}
        {durable && !superseded ? (
          <div
            className="mt-2 rounded-md border border-border p-2 text-2xs"
            data-testid="wiki-recovery-controls"
          >
            <p className="mb-2 text-muted-foreground">
              {generating?.phase === "generation"
                ? "Native generation"
                : "Native recovery"}
              :{" "}
              {failedGeneration
                ? "failed"
                : (generating?.nativeStatus ?? "pending")}
              {generating?.phase !== "generation" &&
              generating?.attempts !== undefined
                ? ` · ${generating.attempts}/5 attempts`
                : ""}
            </p>
            <div className="flex flex-wrap gap-2">
              {onRecoveryRetry &&
              publicationAction &&
              freshness !== "failed" ? (
                <button
                  className="rounded-md bg-primary/15 px-2 py-1"
                  data-testid={`wiki-recovery-publication-action-${name}`}
                  disabled={recoveryPending}
                  onClick={onRecoveryRetry}
                  type="button"
                >
                  {publicationAction}
                </button>
              ) : null}
              {canReconcile ? (
                <button
                  className="rounded-md bg-attention/20 px-2 py-1 text-attention"
                  disabled={recoveryPending}
                  onClick={onRecoveryReconcile}
                  type="button"
                >
                  Reconcile
                </button>
              ) : null}
              {onRecoveryCancel && wikiCanCancelRecovery(generating) ? (
                <button
                  className="rounded-md bg-destructive/10 px-2 py-1 text-destructive"
                  disabled={recoveryPending}
                  onClick={onRecoveryCancel}
                  type="button"
                >
                  Cancel job
                </button>
              ) : null}
              {canRegenerate ? (
                <button
                  className="rounded-md bg-attention/20 px-2 py-1 text-attention"
                  data-testid={`wiki-regenerate-recovery-${name}`}
                  disabled={regeneratePending || recoveryPending}
                  onClick={onRegenerate}
                  type="button"
                >
                  Regenerate from source
                </button>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function formatAge(unix: number): string {
  const delta = Math.max(0, Date.now() / 1000 - unix);
  if (delta < 3600) return `${Math.max(1, Math.round(delta / 60))} minutes ago`;
  if (delta < 86400) return `${Math.round(delta / 3600)} hours ago`;
  return `${Math.round(delta / 86400)} days ago`;
}
