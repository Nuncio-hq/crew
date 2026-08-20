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
import { Button } from "@/shared/ui/button";
import { hermesHomeProfileConfirmSurfaces } from "../lib/hermesProfileBinding";

export function HermesHomeProfileConfirmDialog({
  open,
  onConfirm,
  onOpenChange,
}: {
  open: boolean;
  onConfirm: () => void;
  onOpenChange: (open: boolean) => void;
}) {
  const surfaces = hermesHomeProfileConfirmSurfaces();
  return (
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent data-testid="hermes-home-profile-confirm">
        <AlertDialogHeader>
          <AlertDialogTitle>
            Bind your personal default profile?
          </AlertDialogTitle>
          <AlertDialogDescription>
            This agent will share your personal Hermes home profile (
            <code>default</code> / <code>~/.hermes</code>) including{" "}
            {surfaces.join(", ")}. Crew will not edit that profile. Cancel
            leaves the field unbound.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button
              data-testid="hermes-home-profile-confirm-cancel"
              type="button"
              variant="outline"
            >
              Cancel
            </Button>
          </AlertDialogCancel>
          <AlertDialogAction asChild>
            <Button
              data-testid="hermes-home-profile-confirm-accept"
              onClick={onConfirm}
              type="button"
            >
              Bind personal profile
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
