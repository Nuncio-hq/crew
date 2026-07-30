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

export function CrewProjectWorkspaceConsentDialog({
  canPublish,
  onConfirm,
  onOpenChange,
  onRetryRelay,
  pendingPath,
  relayError,
  relayPending,
  relayUrl,
  saving,
}: {
  canPublish: boolean;
  onConfirm: () => void;
  onOpenChange: (open: boolean) => void;
  onRetryRelay: () => void;
  pendingPath: string | null;
  relayError: boolean;
  relayPending: boolean;
  relayUrl: string | null;
  saving: boolean;
}) {
  return (
    <AlertDialog onOpenChange={onOpenChange} open={pendingPath !== null}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Publish this local path?</AlertDialogTitle>
          <AlertDialogDescription className="space-y-2">
            <span className="block break-all">{pendingPath}</span>
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
                ? "Publish and link"
                : "Waiting for relay"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
