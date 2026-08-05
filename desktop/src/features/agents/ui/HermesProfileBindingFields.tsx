/**
 * Hermes profile binding control + profile-owned model informational row.
 *
 * Rendered from the agent config field model (`hermesProfile` control /
 * `ownedByProfile` omission) — never from a hardcoded runtime id.
 */
import { Input } from "@/shared/ui/input";
import { cn } from "@/shared/lib/cn";
import {
  hermesProfileBindingError,
  profileOwnedModelLabel,
} from "../lib/hermesProfileBinding";
import {
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
} from "./agentConfigOptions";
import { RequiredFieldLabel } from "./agentConfigControls";
import {
  HermesProfileCreateAffordance,
  isNonOwnerOnlyRespondTo,
} from "./HermesProfileCreateAffordance";

export function ProfileOwnedModelRow({
  profileName,
  liveModel,
  className,
}: {
  profileName?: string | null;
  /** Live ACP session model when a clean read path exists; otherwise omit. */
  liveModel?: string | null;
  className?: string;
}) {
  return (
    <div
      className={cn("space-y-1.5", className)}
      data-testid="profile-owned-model-row"
    >
      <p className="text-sm font-medium text-foreground">Model</p>
      <p className="text-sm text-muted-foreground">
        {profileOwnedModelLabel(profileName, liveModel)}
      </p>
    </div>
  );
}

export function HermesProfileField({
  value,
  onChange,
  disabled,
  required = true,
  id = "hermes-profile",
  showValidation = true,
  enableCreateInPlace = true,
  respondTo,
}: {
  value: string;
  onChange: (next: string) => void;
  disabled?: boolean;
  required?: boolean;
  id?: string;
  /** When false, skip inline error (e.g. while typing before blur). */
  showValidation?: boolean;
  /** Phase 03: explicit create button (never silent on save). */
  enableCreateInPlace?: boolean;
  /** When set and not owner-only, show credential-fallback warning. */
  respondTo?: string | null;
}) {
  const error = showValidation
    ? hermesProfileBindingError(value, required)
    : null;
  const showPublicWarning = isNonOwnerOnlyRespondTo(respondTo);

  return (
    <div className="space-y-1.5" data-testid="hermes-profile-field">
      <RequiredFieldLabel htmlFor={id} isRequired={required}>
        Hermes profile
      </RequiredFieldLabel>
      <div
        className={cn(
          "flex min-h-11 items-center px-3",
          PERSONA_FIELD_SHELL_CLASS,
        )}
      >
        <Input
          autoCorrect="off"
          className={cn("h-8 px-0 py-0 leading-6", PERSONA_FIELD_CONTROL_CLASS)}
          disabled={disabled}
          id={id}
          onChange={(event) => onChange(event.target.value)}
          placeholder="scout"
          spellCheck={false}
          value={value}
        />
      </div>
      <p className="text-xs text-muted-foreground">
        Bind this agent to a named Hermes profile (
        <code className="font-mono text-2xs">hermes -p &lt;name&gt;</code>
        ). The manager&apos;s personal{" "}
        <code className="font-mono text-2xs">default</code> profile cannot be
        bound — see docs/crew/HERMES.md.
      </p>
      {enableCreateInPlace ? (
        <HermesProfileCreateAffordance
          disabled={disabled}
          onCreated={onChange}
          profileName={value}
          showPublicAgentWarning={showPublicWarning}
        />
      ) : null}
      {!enableCreateInPlace && showPublicWarning ? (
        <p
          className="text-xs text-muted-foreground"
          data-testid="hermes-public-agent-credential-warning"
        >
          Hermes profiles currently read the manager&apos;s pooled provider
          credentials (see docs/crew/HERMES.md).
        </p>
      ) : null}
      {error ? (
        <p
          className="text-sm text-destructive"
          data-testid="hermes-profile-error"
        >
          {error}
        </p>
      ) : null}
    </div>
  );
}
