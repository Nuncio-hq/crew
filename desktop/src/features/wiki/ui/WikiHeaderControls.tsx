import {
  useWikiGenerate,
  useWikiSetCadence,
} from "@/features/wiki/hooks/useWikiGenerate";
import {
  repoKey,
  wikiFreshness,
  type WikiCadence,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import type { RelayEvent } from "@/shared/api/types";

export function WikiHeaderControls({
  toc,
  owner,
  repoD,
  repoPath,
  onOpenProject,
  onSearchChange,
  search,
  showCadence,
  repoState,
}: {
  toc: WikiToc | null;
  owner?: string;
  repoD?: string;
  repoPath?: string | null;
  onOpenProject?: () => void;
  onSearchChange?: (value: string) => void;
  search?: string;
  showCadence: boolean;
  repoState?: RelayEvent;
}) {
  const setCadence = useWikiSetCadence();
  const generate = useWikiGenerate();
  const cadence = toc?.cadence ?? "manual";
  const freshness = wikiFreshness(toc, repoState);
  return (
    <div className="flex items-center gap-2 text-2xs text-muted-foreground">
      <span
        className={freshness === "stale" ? "text-attention" : undefined}
        data-testid="wiki-freshness"
      >
        {freshness === "stale"
          ? `Stale · Last updated ${toc ? formatAge(toc.generatedAt) : ""}`
          : toc
            ? `Last updated ${formatAge(toc.generatedAt)}`
            : "Never generated"}
      </span>
      <span>⑂ {toc?.branch || "main"}</span>
      {showCadence ? (
        <label className="flex items-center gap-1">
          Auto:
          <select
            aria-label="Wiki cadence"
            className="rounded border border-border bg-background"
            data-testid="wiki-cadence"
            onChange={(event) => {
              if (!toc || !owner || !repoD) return;
              setCadence.mutate({
                owner,
                repoD,
                cadence: event.target.value,
                commit: toc.commit,
                branch: toc.branch,
                sectionsJson: JSON.stringify({ sections: toc.sections }),
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
      {owner && repoD ? (
        <button
          className="rounded-md border border-input bg-muted/40 px-2 py-0.5 text-foreground"
          data-testid="wiki-generate-mirror"
          onClick={() =>
            generate.mutate({
              owner,
              repoD,
              repoKey: repoKey(owner, repoD),
              repoPath,
            })
          }
          type="button"
        >
          {freshness === "stale" ? "Regenerate" : "Generate"}
        </button>
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
