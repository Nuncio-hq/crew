import { Button } from "@/shared/ui/button";

type AgentInstanceEditDialogFooterProps = {
  isAvatarUploadPending: boolean;
  isPending: boolean;
  onCancel: () => void;
  onSubmit: () => void;
  canSubmit: boolean;
};

export function AgentInstanceEditDialogFooter({
  canSubmit,
  isAvatarUploadPending,
  isPending,
  onCancel,
  onSubmit,
}: AgentInstanceEditDialogFooterProps) {
  return (
    <div className="flex w-full items-center justify-end gap-2">
      <Button
        disabled={isPending || isAvatarUploadPending}
        onClick={onCancel}
        type="button"
        variant="outline"
      >
        Cancel
      </Button>
      <Button
        data-testid="edit-agent-dialog-submit"
        disabled={!canSubmit}
        onClick={onSubmit}
        type="button"
      >
        {isPending ? "Saving..." : "Save changes"}
      </Button>
    </div>
  );
}
