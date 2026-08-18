import { truncatePubkey } from "@/shared/lib/pubkey";
import { setTerminalPanelMode } from "@/features/terminal/terminalPanelStore";
import type { WikiJobState } from "@/features/wiki/lib/wikiEvents";

export function WikiRepoCard({
  name,
  owner,
  description,
  freshness,
  generating,
  updatedAt,
  emptyRepo,
  onOpen,
  onGenerate,
}: {
  name: string;
  owner: string;
  description?: string;
  freshness: "never" | "fresh" | "stale" | "generating" | "failed";
  generating?: WikiJobState;
  updatedAt: number | null;
  emptyRepo?: boolean;
  onOpen: () => void;
  onGenerate: () => void;
}) {
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
        {emptyRepo ? (
          <p
            className="text-2xs text-muted-foreground"
            data-testid="wiki-empty-repo"
          >
            Empty repo / no default branch. Push to main, then Generate.
          </p>
        ) : null}
        {!emptyRepo && freshness === "never" ? (
          <button
            className="rounded-md bg-primary px-2 py-1 text-2xs text-primary-foreground"
            data-testid={`wiki-generate-${name}`}
            onClick={onGenerate}
            type="button"
          >
            Generate wiki
          </button>
        ) : null}
        {freshness === "generating" ? (
          <div
            className="text-2xs text-muted-foreground"
            data-testid="wiki-generating"
          >
            Generating… {generating?.done ?? 0}/{generating?.total ?? 0} pages
            {generating?.costNote ? (
              <div className="mt-1">{generating.costNote}</div>
            ) : null}
          </div>
        ) : null}
        {freshness === "fresh" && updatedAt ? (
          <div className="text-2xs text-muted-foreground">
            ⏱ {formatAge(updatedAt)}
          </div>
        ) : null}
        {freshness === "stale" ? (
          <button
            className="rounded-md bg-attention/20 px-2 py-1 text-2xs text-attention"
            data-testid={`wiki-regenerate-${name}`}
            onClick={onGenerate}
            type="button"
          >
            Stale · Regenerate
          </button>
        ) : null}
        {freshness === "failed" ? (
          <div data-testid="wiki-failed">
            <p className="mb-1 text-2xs text-destructive">
              {generating?.error ?? "Generation failed"}
            </p>
            <div className="flex gap-2">
              <button
                className="rounded-md bg-destructive/15 px-2 py-1 text-2xs text-destructive"
                onClick={() => setTerminalPanelMode("docked")}
                type="button"
              >
                logs → Term
              </button>
              <button
                className="rounded-md bg-destructive/15 px-2 py-1 text-2xs text-destructive"
                onClick={onGenerate}
                type="button"
              >
                Retry
              </button>
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
