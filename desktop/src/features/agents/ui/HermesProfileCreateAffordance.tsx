/**
 * Explicit create-in-place affordance for Hermes profiles (S-2.1 / P-6).
 * Never runs on save — manager must click the button.
 */
import * as React from "react";

import {
  createHermesProfile,
  hermesProfileCreateCommandLine,
  hermesProfileLifecycleMessage,
  hermesProfileLifecycleSuccess,
} from "@/shared/api/hermesProfiles";
import { Button } from "@/shared/ui/button";
import { validateHermesProfileName } from "../lib/hermesProfileBinding";

export function HermesProfileCreateAffordance({
  profileName,
  disabled,
  showPublicAgentWarning = false,
  onCreated,
}: {
  profileName: string;
  disabled?: boolean;
  showPublicAgentWarning?: boolean;
  onCreated: (name: string) => void;
}) {
  const trimmed = profileName.trim();
  const validationError = validateHermesProfileName(trimmed);
  const canCreate = trimmed.length > 0 && validationError == null;
  const [pending, setPending] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  if (!canCreate && !error) {
    return showPublicAgentWarning ? <PublicAgentCredentialWarning /> : null;
  }

  async function handleCreate() {
    if (!canCreate || pending) return;
    setPending(true);
    setError(null);
    try {
      const result = await createHermesProfile(trimmed);
      if (hermesProfileLifecycleSuccess(result)) {
        onCreated(trimmed);
      } else {
        setError(hermesProfileLifecycleMessage(result));
      }
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to create Hermes profile.",
      );
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="space-y-1.5" data-testid="hermes-profile-create-affordance">
      {canCreate ? (
        <>
          <Button
            data-testid="hermes-profile-create-button"
            disabled={disabled || pending}
            onClick={() => void handleCreate()}
            size="sm"
            type="button"
            variant="secondary"
          >
            {pending ? "Creating…" : `Create profile '${trimmed}'`}
          </Button>
          <p className="font-mono text-2xs text-muted-foreground">
            runs: {hermesProfileCreateCommandLine(trimmed)}
          </p>
        </>
      ) : null}
      {error ? (
        <p
          className="text-sm text-destructive"
          data-testid="hermes-profile-create-error"
        >
          {error}
        </p>
      ) : null}
      {showPublicAgentWarning ? <PublicAgentCredentialWarning /> : null}
    </div>
  );
}

function PublicAgentCredentialWarning() {
  return (
    <p
      className="text-xs text-muted-foreground"
      data-testid="hermes-public-agent-credential-warning"
    >
      Hermes profiles currently read the manager&apos;s pooled provider
      credentials (see docs/crew/HERMES.md). Prefer owner-only respond-to until
      per-profile isolation exists.
    </p>
  );
}

/** True when respond-to is anyone or allowlist (not owner-only / unset). */
export function isNonOwnerOnlyRespondTo(
  respondTo: string | null | undefined,
): boolean {
  return respondTo === "anyone" || respondTo === "allowlist";
}
