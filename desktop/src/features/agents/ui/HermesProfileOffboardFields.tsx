/**
 * Keep vs delete Hermes profile choice for agent offboarding (C-13 / C-14).
 * Profile-delete is never preselected.
 */
import { hermesProfileDeleteCommandLine } from "@/shared/api/hermesProfiles";

export type HermesProfileOffboardChoice = "keep" | "delete";

export function HermesProfileOffboardFields({
  profileName,
  choice,
  onChoiceChange,
  showPublicAgentWarning = false,
}: {
  profileName: string;
  choice: HermesProfileOffboardChoice;
  onChoiceChange: (next: HermesProfileOffboardChoice) => void;
  /** Display-only caveat when respond-to ≠ owner-only (spike 0010). */
  showPublicAgentWarning?: boolean;
}) {
  const name = profileName.trim();
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
          checked={choice === "delete"}
          className="mt-0.5"
          data-testid="hermes-profile-offboard-delete"
          name="hermes-profile-offboard"
          onChange={() => onChoiceChange("delete")}
          type="radio"
          value="delete"
        />
        <span>
          Also delete the profile
          <span className="mt-0.5 block font-mono text-2xs text-muted-foreground">
            runs: {hermesProfileDeleteCommandLine(name)}
          </span>
        </span>
      </label>
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
