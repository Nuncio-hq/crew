import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderOpen } from "lucide-react";
import { toast } from "sonner";

import { useProjectsQuery } from "@/features/projects/hooks";
import {
  projectLocalWorkspaceFromEvent,
  readCanonicalProjectChannel,
} from "@/features/projects/lib/project-local-workspace";
import {
  projectWorkspaceUiReadiness,
  reusableProjectWorkspaceChannel,
  type RetryProjectChannel,
} from "@/features/projects/lib/project-local-workspace-ui";
import {
  createProjectWorkspaceChannel,
  currentRelayWsUrl,
  fetchCurrentProjectAnnouncement,
  linkCurrentProjectWorkspace,
} from "@/features/projects/lib/project-local-workspace-runtime";
import { ProjectsListScopeDropdown } from "@/features/projects/ui/ProjectsListScopeDropdown";
import { CrewProjectWorkspaceConsentDialog } from "@/features/projects/ui/crew-project-workspace-consent-dialog";
import { CrewProjectWorkspaceStatus } from "@/features/projects/ui/crew-project-workspace-status";
import { useIdentityQuery } from "@/shared/api/hooks";
import { chooseProjectWorkspaceFolder } from "@/shared/api/tauri-project-folder-dialog";
import { Button } from "@/shared/ui/button";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Could not link workspace.";
}

export function CrewProjectWorkspacePanel() {
  const projectsQuery = useProjectsQuery();
  const identityQuery = useIdentityQuery();
  const queryClient = useQueryClient();
  const [selectedId, setSelectedId] = React.useState("");
  const [pendingPath, setPendingPath] = React.useState<string | null>(null);
  const [selectedNowPath, setSelectedNowPath] = React.useState<string | null>(
    null,
  );
  const [saving, setSaving] = React.useState(false);
  const [retryChannel, setRetryChannel] =
    React.useState<RetryProjectChannel | null>(null);
  const currentPubkey = identityQuery.data?.pubkey.toLowerCase();
  const projects = React.useMemo(
    () =>
      (projectsQuery.data ?? []).filter(
        (project) => project.owner.toLowerCase() === currentPubkey,
      ),
    [currentPubkey, projectsQuery.data],
  );
  const selected =
    projects.find((project) => project.id === selectedId) ??
    projects[0] ??
    null;

  React.useEffect(() => {
    if (selected && selected.id !== selectedId) {
      setSelectedId(selected.id);
      setRetryChannel(null);
      setSelectedNowPath(null);
    }
  }, [selected, selectedId]);

  const announcementQuery = useQuery({
    enabled: Boolean(selected),
    queryKey: ["crew-project-announcement", selected?.owner, selected?.dtag],
    queryFn: () => {
      if (!selected) throw new Error("No Project selected.");
      return fetchCurrentProjectAnnouncement(selected.owner, selected.dtag);
    },
  });
  const relayUrlQuery = useQuery({
    queryKey: ["crew-project-workspace-relay-url"],
    queryFn: currentRelayWsUrl,
  });
  const workspace = announcementQuery.data
    ? projectLocalWorkspaceFromEvent(announcementQuery.data).localWorkspace
    : { status: "unlinked" as const };
  const announcementStatus = announcementQuery.isPending
    ? "loading"
    : announcementQuery.isError
      ? "error"
      : announcementQuery.data
        ? "ready"
        : "missing";
  const relayUrl = relayUrlQuery.data ?? null;
  const readiness = projectWorkspaceUiReadiness({
    announcementStatus,
    relayUrl,
  });

  const chooseFolder = async () => {
    try {
      const path = await chooseProjectWorkspaceFolder();
      if (path) setPendingPath(path);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  };

  const confirmLink = async () => {
    if (!selected || !pendingPath || !currentPubkey) return;
    setSaving(true);
    let createdChannelId: string | null = null;
    try {
      if (!readiness.canPublish || !relayUrl) {
        throw new Error(
          "Resolve the exact relay destination before publishing.",
        );
      }
      if (selected.owner.toLowerCase() !== currentPubkey) {
        throw new Error("Only the Project owner can link a workspace.");
      }
      const current = await fetchCurrentProjectAnnouncement(
        selected.owner,
        selected.dtag,
      );
      if (!current) throw new Error("Project announcement not found on relay.");
      const channel = readCanonicalProjectChannel(current.tags);
      if (channel.status === "invalid") {
        throw new Error("Project has an invalid canonical Project channel.");
      }
      let channelId = reusableProjectWorkspaceChannel(
        selected.id,
        channel.status === "ready" ? channel.channelId : null,
        retryChannel,
      );
      if (!channelId) {
        channelId = await createProjectWorkspaceChannel(selected.name);
        setRetryChannel({ projectId: selected.id, channelId });
      }
      if (channel.status === "absent") createdChannelId = channelId;
      const saved = await linkCurrentProjectWorkspace({
        owner: selected.owner,
        currentPubkey,
        dtag: selected.dtag,
        channelId,
        localPath: pendingPath,
      });
      queryClient.setQueryData(
        ["crew-project-announcement", selected.owner, selected.dtag],
        saved,
      );
      setSelectedNowPath(pendingPath);
      setRetryChannel(null);
      setPendingPath(null);
      toast.success("Project workspace linked on relay.");
    } catch (error) {
      const suffix = createdChannelId
        ? ` Channel ${createdChannelId} was created and can be reused on retry.`
        : "";
      toast.error(`${errorMessage(error)}${suffix}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="flex flex-wrap items-center gap-3 border-b px-4 py-2">
      <span className="text-sm font-semibold">Local workspace</span>
      {projects.length > 0 && selected ? (
        <ProjectsListScopeDropdown
          label="Project for local workspace"
          onChange={(projectId) => {
            setSelectedId(projectId);
            setRetryChannel(null);
            setSelectedNowPath(null);
          }}
          options={projects.map((project) => ({
            label: project.name,
            value: project.id,
          }))}
          value={selected.id}
        />
      ) : (
        <span className="text-sm text-muted-foreground">
          Create or register a Project you own first.
        </span>
      )}
      {selected ? (
        <>
          <CrewProjectWorkspaceStatus
            announcementStatus={announcementStatus}
            onRetry={() => announcementQuery.refetch()}
            selectedNowPath={selectedNowPath}
            workspace={workspace}
          />
          <Button
            disabled={!readiness.canChooseFolder}
            onClick={chooseFolder}
            size="sm"
            variant="outline"
          >
            <FolderOpen className="h-4 w-4" />
            {workspace.status === "linked" ? "Relink folder" : "Link folder"}
          </Button>
        </>
      ) : null}
      <CrewProjectWorkspaceConsentDialog
        canPublish={readiness.canPublish}
        onConfirm={confirmLink}
        onOpenChange={(open) => !open && !saving && setPendingPath(null)}
        onRetryRelay={() => relayUrlQuery.refetch()}
        pendingPath={pendingPath}
        relayError={relayUrlQuery.isError}
        relayPending={relayUrlQuery.isPending || relayUrlQuery.isFetching}
        relayUrl={relayUrl}
        saving={saving}
      />
    </section>
  );
}
