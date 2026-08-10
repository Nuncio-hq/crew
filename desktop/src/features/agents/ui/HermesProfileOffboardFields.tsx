/**
 * Keep vs delete Hermes profile choice for agent offboarding (C-13 / C-14).
 * Profile-delete is never preselected.
 */
import * as React from "react";
import {
  estimateHermesProfileArchive,
  type HermesProfileArchiveEstimate,
} from "@/shared/api/hermesProfiles";

export type HermesProfileOffboardChoice = "keep" | "archive";

export function formatArchiveBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes;
  let unit = "B";
  for (const next of units) {
    value /= 1024;
    unit = next;
    if (value < 1024) break;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${unit}`;
}

export function HermesProfileOffboardFields({
  profileName,
  choice,
  onChoiceChange,
  reason = "",
  onReasonChange,
  isRunning = false,
  showPublicAgentWarning = false,
}: {
  profileName: string;
  choice: HermesProfileOffboardChoice;
  onChoiceChange: (next: HermesProfileOffboardChoice) => void;
  reason?: string;
  onReasonChange?: (next: string) => void;
  isRunning?: boolean;
  /** Display-only caveat when respond-to ≠ owner-only (spike 0010). */
  showPublicAgentWarning?: boolean;
}) {
  const name = profileName.trim();
  const [estimate, setEstimate] =
    React.useState<HermesProfileArchiveEstimate | null>(null);
  const [estimateError, setEstimateError] = React.useState<string | null>(null);
  React.useEffect(() => {
    let active = true;
    setEstimateError(null);
    void estimateHermesProfileArchive(name)
      .then((value) => {
        if (active) setEstimate(value);
      })
      .catch((error) => {
        if (active) {
          setEstimateError(
            error instanceof Error ? error.message : "Estimate unavailable.",
          );
        }
      });
    return () => {
      active = false;
    };
  }, [name]);
  if (!name) return null;

  return (
    <fieldset
      className="space-y-2 rounded-lg border border-border p-3"
      data-testid="hermes-profile-offboard-choice"
    >
      <legend className="px-1 text-sm font-medium text-foreground">
        Hermes profile &lsquo;{name}&rsquo;
      </legend>
      <label className="flex cursor-pointer items-start gap-2 text-sm text-foreground">
        <input
          checked={choice === "keep"}
          className="mt-0.5"
          data-testid="hermes-profile-offboard-keep"
          name="hermes-profile-offboard"
          onChange={() => onChoiceChange("keep")}
          type="radio"
          value="keep"
        />
        <span>
          Keep profile &lsquo;{name}&rsquo; (memory + skills)
          <span className="mt-0.5 block text-xs text-muted-foreground">
            Default — re-attach later by binding the same name.
          </span>
        </span>
      </label>
      <label className="flex cursor-pointer items-start gap-2 text-sm text-foreground">
        <input
          checked={choice === "archive"}
          disabled={isRunning}
          className="mt-0.5"
          data-testid="hermes-profile-offboard-delete"
          name="hermes-profile-offboard"
          onChange={() => onChoiceChange("archive")}
          type="radio"
          value="archive"
        />
        <span>
          Archive the profile
          <span className="mt-0.5 block font-mono text-2xs text-muted-foreground">
            Moves memories and skills into Crew&apos;s restorable archive;
            caches are excluded.
          </span>
        </span>
      </label>
      {isRunning ? (
        <p className="text-xs text-muted-foreground">
          Stop the running agent before archiving its profile.
        </p>
      ) : null}
      {choice === "archive" ? (
        <div className="space-y-2">
          <p className="text-xs text-muted-foreground">
            The live profile leaves ~/.hermes/profiles/{name}; memories and
            skills are preserved and can be restored later.
          </p>
          {estimate ? (
            <p
              className="text-xs text-muted-foreground"
              data-testid="hermes-profile-archive-estimate"
            >
              Estimated archive: {formatArchiveBytes(estimate.included_bytes)}{" "}
              included; {formatArchiveBytes(estimate.excluded_bytes)} excluded (
              {estimate.entry_count} entries). Excludes:{" "}
              {estimate.excluded_bytes > 0 ? "cache directories" : "none found"}
              .
            </p>
          ) : estimateError ? (
            <p className="text-xs text-muted-foreground">
              Archive size estimate unavailable: {estimateError}
            </p>
          ) : (
            <p className="text-xs text-muted-foreground">
              Estimating archive size…
            </p>
          )}
          {onReasonChange ? (
            <textarea
              aria-label="Optional archive reason"
              className="min-h-16 w-full rounded-md border border-input bg-background px-2 py-1.5 text-sm"
              data-testid="hermes-profile-offboard-reason"
              onChange={(event) => onReasonChange(event.target.value)}
              placeholder="Optional reason for archiving"
              value={reason}
            />
          ) : null}
        </div>
      ) : null}
      {showPublicAgentWarning ? (
        <p
          className="text-xs text-muted-foreground"
          data-testid="hermes-public-agent-credential-warning"
        >
          Hermes profiles currently read the manager&apos;s pooled provider
          credentials (see docs/crew/HERMES.md).
        </p>
      ) : null}
    </fieldset>
  );
}
