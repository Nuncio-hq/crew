import * as React from "react";
import {
  listHermesProfileArchives,
  permanentlyDeleteHermesProfileArchive,
  restoreHermesProfileArchive,
  type HermesProfileArchiveListing,
} from "@/shared/api/hermesProfiles";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { formatArchiveBytes } from "./HermesProfileOffboardFields";
import { requestOpenEditAgent } from "@/features/agents/openEditAgentEvent";
import type { ManagedAgent } from "@/shared/api/types";

function archiveResultMessage(
  result: Awaited<ReturnType<typeof restoreHermesProfileArchive>>,
): string {
  switch (result.status) {
    case "restored":
      return `Profile '${result.profile}' restored.`;
    case "collision":
    case "agent_running":
    case "failed":
    case "does_not_exist":
    case "invalid_name":
      return result.message;
    default:
      return "Archive operation failed.";
  }
}

export function HermesProfileArchivesPanel({
  open,
  onOpenChange,
  managedAgents = [],
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  managedAgents?: readonly ManagedAgent[];
}) {
  const [archives, setArchives] = React.useState<HermesProfileArchiveListing[]>(
    [],
  );
  const [error, setError] = React.useState<string | null>(null);
  const [busyId, setBusyId] = React.useState<string | null>(null);
  const [confirmId, setConfirmId] = React.useState<string | null>(null);
  const [token, setToken] = React.useState("");
  const [restored, setRestored] = React.useState<string | null>(null);

  const refresh = React.useCallback(async () => {
    try {
      setError(null);
      const entries = await listHermesProfileArchives();
      setArchives(
        [...entries].sort((a, b) =>
          b.manifest.archived_at.localeCompare(a.manifest.archived_at),
        ),
      );
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Archives unavailable.",
      );
    }
  }, []);

  React.useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  async function restore(id: string) {
    setBusyId(id);
    setError(null);
    try {
      const result = await restoreHermesProfileArchive(id);
      if (result.status === "restored") {
        setRestored(id);
        await refresh();
      } else {
        setError(archiveResultMessage(result));
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Restore failed.");
    } finally {
      setBusyId(null);
    }
  }

  async function permanentlyDelete(id: string) {
    setBusyId(id);
    setError(null);
    try {
      const result = await permanentlyDeleteHermesProfileArchive(id, token);
      if (result.status === "permanently_deleted") {
        setConfirmId(null);
        setToken("");
        await refresh();
      } else {
        setError(archiveResultMessage(result));
      }
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Permanent deletion failed.",
      );
    } finally {
      setBusyId(null);
    }
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className="max-w-2xl"
        data-testid="hermes-profile-archives"
      >
        <DialogHeader>
          <DialogTitle>Archived Hermes profiles</DialogTitle>
          <DialogDescription>
            Restorable Crew-owned copies of profile memories and skills. Cache
            directories are excluded from every archive.
          </DialogDescription>
        </DialogHeader>
        {error ? (
          <p
            className="text-sm text-destructive"
            data-testid="hermes-profile-archives-error"
          >
            {error}
          </p>
        ) : null}
        {archives.length === 0 ? (
          <p
            className="text-sm text-muted-foreground"
            data-testid="hermes-profile-archives-empty"
          >
            No archived Hermes profiles.
          </p>
        ) : (
          <div className="max-h-[60vh] space-y-3 overflow-y-auto">
            {archives.map(({ id, archive_bytes, manifest }) => (
              <article
                className="space-y-2 rounded-lg border border-border p-3"
                data-testid={`hermes-profile-archive-row-${id}`}
                key={id}
              >
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <h3 className="font-medium">{manifest.profile}</h3>
                    <p className="text-xs text-muted-foreground">
                      {new Date(manifest.archived_at).toLocaleString()} ·{" "}
                      {formatArchiveBytes(archive_bytes)}
                    </p>
                  </div>
                  <div className="flex gap-2">
                    <Button
                      data-testid={`hermes-profile-archive-restore-${id}`}
                      disabled={busyId !== null}
                      onClick={() => void restore(id)}
                      size="sm"
                      variant="outline"
                    >
                      Restore
                    </Button>
                    <Button
                      data-testid={`hermes-profile-archive-delete-${id}`}
                      disabled={busyId !== null}
                      onClick={() => {
                        setConfirmId(id);
                        setToken("");
                      }}
                      size="sm"
                      variant="destructive"
                    >
                      Permanently delete
                    </Button>
                  </div>
                </div>
                <p className="text-xs text-muted-foreground">
                  Bound agent: {manifest.bound_agent_name ?? "none"}
                  {manifest.bound_agent_pubkey
                    ? ` (${manifest.bound_agent_pubkey})`
                    : ""}
                  {" · "}Reason: {manifest.offboard_reason ?? "not provided"}
                  {" · "}Excludes: {manifest.exclusions.join(", ") || "none"}
                  {manifest.skipped_links.length > 0
                    ? ` · ${manifest.skipped_links.length} links omitted`
                    : ""}
                </p>
                {confirmId === id ? (
                  <div className="space-y-2 rounded-md bg-muted p-2">
                    <p className="text-xs text-destructive">
                      This permanently destroys the archived memories and
                      skills. Type &lsquo;{manifest.profile}&rsquo; to confirm.
                    </p>
                    <input
                      aria-label={`Type ${manifest.profile} to permanently delete`}
                      className="w-full rounded-md border border-input bg-background px-2 py-1.5 text-sm"
                      data-testid={`hermes-profile-archive-confirm-input-${id}`}
                      onChange={(event) => setToken(event.target.value)}
                      value={token}
                    />
                    <Button
                      data-testid={`hermes-profile-archive-confirm-delete-${id}`}
                      disabled={token !== manifest.profile || busyId !== null}
                      onClick={() => void permanentlyDelete(id)}
                      size="sm"
                      variant="destructive"
                    >
                      Confirm permanent deletion
                    </Button>
                  </div>
                ) : null}
                {restored === id ? (
                  manifest.bound_agent_pubkey &&
                  managedAgents.some(
                    (agent) => agent.pubkey === manifest.bound_agent_pubkey,
                  ) ? (
                    <button
                      className="text-xs font-medium text-muted-foreground hover:underline"
                      data-testid={`hermes-profile-archive-rebind-${id}`}
                      onClick={() => {
                        const pubkey = manifest.bound_agent_pubkey;
                        if (!pubkey) return;
                        requestOpenEditAgent(pubkey, {
                          type: "normalized_field",
                          field: "hermesProfile",
                        });
                      }}
                      type="button"
                    >
                      Re-bind restored profile to {manifest.bound_agent_name}
                    </button>
                  ) : (
                    <p
                      className="text-xs text-muted-foreground"
                      data-testid={`hermes-profile-archive-rebind-help-${id}`}
                    >
                      Profile restored. Bind it from the agent&apos;s Edit Agent
                      profile field.
                    </p>
                  )
                ) : null}
              </article>
            ))}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
