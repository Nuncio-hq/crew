import type { LocalWorkspaceState } from "@/features/projects/lib/project-local-workspace";
import type { ProjectAnnouncementUiStatus } from "@/features/projects/lib/project-local-workspace-ui";
import { Button } from "@/shared/ui/button";

export function CrewProjectWorkspaceStatus({
  announcementStatus,
  onRetry,
  selectedNowPath,
  workspace,
}: {
  announcementStatus: ProjectAnnouncementUiStatus;
  onRetry: () => void;
  selectedNowPath: string | null;
  workspace: LocalWorkspaceState;
}) {
  if (announcementStatus === "loading") {
    return (
      <span className="text-sm text-muted-foreground">
        Loading Project from relay…
      </span>
    );
  }
  if (announcementStatus === "error" || announcementStatus === "missing") {
    return (
      <>
        <span className="text-sm text-muted-foreground">
          {announcementStatus === "error"
            ? "Could not load Project from relay"
            : "Project announcement not found on relay"}
        </span>
        <Button onClick={onRetry} size="sm" variant="ghost">
          Retry
        </Button>
      </>
    );
  }
  if (workspace.status === "invalid") {
    return (
      <span className="text-sm text-muted-foreground">
        Invalid relay workspace metadata
      </span>
    );
  }
  if (workspace.status === "unlinked") {
    return <span className="text-sm text-muted-foreground">Not linked</span>;
  }
  return (
    <span
      className="max-w-xl truncate text-sm text-muted-foreground"
      title={workspace.path}
    >
      {workspace.path} —{" "}
      {selectedNowPath === workspace.path
        ? "selected now"
        : "not locally verified"}
    </span>
  );
}
