import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";

type ChannelWorktreesRemoveDialogProps = {
  paths: string[] | null;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (paths: string[]) => void;
};

export function ChannelWorktreesRemoveDialog({
  paths,
  busy,
  onCancel,
  onConfirm,
}: ChannelWorktreesRemoveDialogProps) {
  return (
    <AlertDialog
      open={paths !== null}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Free local space?</AlertDialogTitle>
          <AlertDialogDescription>
            {paths?.length ?? 0} checkout
            {(paths?.length ?? 0) === 1 ? "" : "s"} will be evicted. Dirty
            worktrees and checkouts with ignored local files are refused — clear
            generated cache or review local files first. Branches are kept; a
            later agent turn reattaches a clean checkout.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <ul className="max-h-40 space-y-1 overflow-auto text-2xs text-muted-foreground">
          {paths?.map((path) => (
            <li key={path} className="break-all">
              {path}
            </li>
          ))}
        </ul>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            disabled={busy}
            onClick={() => {
              if (paths) onConfirm(paths);
            }}
          >
            Free local space
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
