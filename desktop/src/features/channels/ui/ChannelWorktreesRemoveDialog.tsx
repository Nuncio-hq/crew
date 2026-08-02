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
          <AlertDialogTitle>Remove worktrees?</AlertDialogTitle>
          <AlertDialogDescription>
            {paths?.length ?? 0} path
            {(paths?.length ?? 0) === 1 ? "" : "s"} will be removed. Dirty
            worktrees are refused. Branches are kept for orphans.
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
            Remove
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
