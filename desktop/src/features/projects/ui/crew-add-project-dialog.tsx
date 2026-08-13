import { localWorkspacePrivacyNotice } from "@/features/projects/lib/project-local-workspace";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

export function CrewAddProjectDialog({
  cowork = false,
  localPath,
  name,
  onConfirm,
  onNameChange,
  onOpenChange,
  onRetryRelay,
  relayError,
  relayPending,
  relayUrl,
  saving,
}: {
  cowork?: boolean;
  localPath: string | null;
  name: string;
  onConfirm: () => void;
  onNameChange: (name: string) => void;
  onOpenChange: (open: boolean) => void;
  onRetryRelay: () => void;
  relayError: boolean;
  relayPending: boolean;
  relayUrl: string | null;
  saving: boolean;
}) {
  const canPublish = Boolean(localPath && name.trim() && relayUrl);

  return (
    <AlertDialog onOpenChange={onOpenChange} open={localPath !== null}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {cowork ? "Add this Cowork Project?" : "Add this Repository?"}
          </AlertDialogTitle>
          <AlertDialogDescription className="space-y-3">
            <span className="block">
              {cowork
                ? "Agents will work in this folder. NuncioCrew keeps a private version history outside the folder — the folder itself stays unchanged."
                : "The folder stays where it is. NuncioCrew will not clone, initialize Git, or change its files."}
            </span>
            <span className="block break-all font-mono text-xs">
              {localPath}
            </span>
            {relayUrl ? (
              <span className="block">
                {localWorkspacePrivacyNotice(relayUrl)}
              </span>
            ) : (
              <span className="block">
                {relayError
                  ? "Could not resolve the relay destination."
                  : "Resolving the relay destination…"}
              </span>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <label className="space-y-2 text-sm" htmlFor="crew-project-name">
          <span className="font-medium">
            {cowork ? "Project name" : "Repository name"}
          </span>
          <Input
            autoFocus
            disabled={saving}
            id="crew-project-name"
            onChange={(event) => onNameChange(event.target.value)}
            value={name}
          />
        </label>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={saving}>Cancel</AlertDialogCancel>
          {relayError && !relayPending ? (
            <Button onClick={onRetryRelay} variant="outline">
              Retry relay
            </Button>
          ) : null}
          <Button disabled={saving || !canPublish} onClick={onConfirm}>
            {saving
              ? "Publishing…"
              : canPublish
                ? cowork
                  ? "Add Cowork Project"
                  : "Add Repository"
                : "Waiting for relay"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
