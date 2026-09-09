import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  loadProjectChannelLink,
  retryProjectWorkspaceLink,
  retryProjectWorkspaceUnlink,
} from "@/shared/api/projectChannelLink";
import { Button } from "@/shared/ui/button";

/**
 * Renders the recovery affordance for a pending workspace operation.
 *
 * The native journal is the authority; this component only loads the exact
 * resource claim and retries its persisted event. It is mounted beside the
 * channel chip and thread workspace drawer, which are the reachable folder
 * actions in the production shell.
 */
export function ProjectWorkspaceRecoveryControl({
  onRecovered,
  repositoryCoordinate,
}: {
  onRecovered?: () => Promise<void> | void;
  repositoryCoordinate: string;
}) {
  const recoveryQuery = useQuery({
    enabled: repositoryCoordinate.length > 0,
    queryKey: ["project-workspace-unlink-recovery", repositoryCoordinate],
    queryFn: () => loadProjectChannelLink(repositoryCoordinate),
    retry: false,
    staleTime: 0,
  });
  const recovery = recoveryQuery.data;
  const operation = recovery?.operation;
  const action = operation?.payload.action;
  if (
    !recovery ||
    !operation ||
    !action ||
    (action.type !== "link-workspace" && action.type !== "unlink-workspace")
  ) {
    return null;
  }

  /*
   * Keep the success affordance behind the driver's terminal proof. A native
   * dispatch can resolve with a failed or superseded operation when the relay
   * gate is unavailable; that is a durable recovery state, not a success.
   */
  const retry = async () => {
    const result =
      action.type === "link-workspace"
        ? await retryProjectWorkspaceLink(
            recovery.token,
            operation,
            repositoryCoordinate,
            action.channel_id,
            action.local_path,
          )
        : await retryProjectWorkspaceUnlink(
            recovery.token,
            operation,
            repositoryCoordinate,
          );
    if (!result.value.reconciled || result.value.status !== "complete") {
      throw new Error(
        result.value.payload.last_error ??
          (action.type === "link-workspace"
            ? "The workspace link is still pending relay confirmation."
            : "The workspace unlink is still pending relay confirmation."),
      );
    }
    await recoveryQuery.refetch();
    await onRecovered?.();
    toast.success(
      action.type === "link-workspace"
        ? "Project workspace link recovered."
        : "Project workspace unlink recovered.",
    );
  };

  return (
    <Button
      aria-label={
        action.type === "link-workspace"
          ? "Retry workspace link"
          : "Retry workspace unlink"
      }
      data-testid="project-workspace-recovery"
      disabled={recoveryQuery.isFetching}
      onClick={() =>
        void retry().catch((error) =>
          toast.error(
            error instanceof Error
              ? error.message
              : "Could not recover the workspace operation.",
          ),
        )
      }
      size="sm"
      type="button"
      variant="ghost"
    >
      {action.type === "link-workspace" ? "Retry link" : "Retry unlink"}
    </Button>
  );
}
