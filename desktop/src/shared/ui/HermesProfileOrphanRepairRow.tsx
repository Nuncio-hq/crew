/**
 * Orphan Hermes profile repair row for config-nudge cards (C-03 extension).
 */
import * as React from "react";

import {
  createHermesProfile,
  hermesProfileCreateCommandLine,
  hermesProfileLifecycleMessage,
  hermesProfileLifecycleSuccess,
} from "@/shared/api/hermesProfiles";
import type { EditAgentFocusTarget } from "@/features/agents/openEditAgentEvent";

export function HermesProfileOrphanRepairRow({
  profile,
  onOpenEditAgent,
}: {
  profile: string;
  onOpenEditAgent: (
    e: React.MouseEvent,
    focus: EditAgentFocusTarget | undefined,
  ) => void;
}) {
  const [pending, setPending] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [done, setDone] = React.useState(false);

  async function handleRecreate(e: React.MouseEvent) {
    e.stopPropagation();
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      const result = await createHermesProfile(profile);
      if (hermesProfileLifecycleSuccess(result)) {
        setDone(true);
      } else {
        setError(hermesProfileLifecycleMessage(result));
      }
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to recreate profile.",
      );
    } finally {
      setPending(false);
    }
  }

  return (
    <div
      className="space-y-1.5 text-xs leading-4 text-muted-foreground"
      data-testid="hermes-profile-orphan-repair"
    >
      <span className="block [overflow-wrap:anywhere]">
        Hermes profile{" "}
        <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs text-foreground">
          {profile}
        </code>{" "}
        is missing on disk.
        {done ? " Recreated — restart the agent." : null}
      </span>
      {!done ? (
        <div className="flex flex-wrap items-center gap-3">
          <button
            className="relative z-20 font-medium text-muted-foreground hover:underline"
            data-testid="hermes-profile-recreate-button"
            disabled={pending}
            onClick={(e) => void handleRecreate(e)}
            type="button"
          >
            {pending ? "Recreating…" : `Recreate profile '${profile}'`}
          </button>
          <button
            className="relative z-20 font-medium text-muted-foreground hover:underline"
            data-testid="hermes-profile-change-binding"
            onClick={(e) =>
              onOpenEditAgent(e, {
                type: "normalized_field",
                field: "hermesProfile",
              })
            }
            type="button"
          >
            Change binding →
          </button>
        </div>
      ) : null}
      <p className="font-mono text-2xs text-muted-foreground">
        runs: {hermesProfileCreateCommandLine(profile)}
      </p>
      {error ? (
        <p
          className="text-destructive"
          data-testid="hermes-profile-recreate-error"
        >
          {error}
        </p>
      ) : null}
    </div>
  );
}
