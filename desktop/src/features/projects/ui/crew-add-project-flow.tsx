import { useQuery, useQueryClient } from "@tanstack/react-query";
import * as React from "react";
import { toast } from "sonner";

import {
  projectNameFromLocalPath,
  ProjectLocalWorkspaceCreateError,
  type ProjectChannelRetry,
} from "@/features/projects/lib/project-add-local-workspace";
import { createCurrentLocalWorkspaceProject } from "@/features/projects/lib/project-add-local-workspace-runtime";
import { currentRelayWsUrl } from "@/features/projects/lib/project-local-workspace-runtime";
import { type Project, projectsQueryKey } from "@/features/projects/hooks";
import { CrewAddProjectDialog } from "@/features/projects/ui/crew-add-project-dialog";
import { chooseProjectWorkspaceFolder } from "@/shared/api/tauri-project-folder-dialog";
import { probeProjectGitWorkspace } from "@/shared/api/projectGitWorkspaceProbe";
import { initCoworkHistory } from "@/shared/api/coworkVersions";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Could not add Repository.";
}

export function CrewAddProjectFlow({
  children,
}: {
  children: (chooseFolder: () => void) => React.ReactNode;
}) {
  const queryClient = useQueryClient();
  const [localPath, setLocalPath] = React.useState<string | null>(null);
  const [name, setName] = React.useState("");
  const [cowork, setCowork] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const [retryChannel, setRetryChannel] =
    React.useState<ProjectChannelRetry | null>(null);
  const relayUrlQuery = useQuery({
    queryKey: ["crew-project-workspace-relay-url"],
    queryFn: currentRelayWsUrl,
  });

  const chooseFolder = React.useCallback(async () => {
    try {
      const path = await chooseProjectWorkspaceFolder();
      if (!path) return;
      const probe = await probeProjectGitWorkspace(path);
      setCowork(!probe.isGit);
      setLocalPath(path);
      setName(projectNameFromLocalPath(path));
      setRetryChannel(null);
    } catch (error) {
      toast.error(errorMessage(error));
    }
  }, []);

  const close = () => {
    if (saving) return;
    setLocalPath(null);
    setName("");
    setCowork(false);
    setRetryChannel(null);
  };

  const confirm = async () => {
    if (!localPath || !relayUrlQuery.data) return;
    setSaving(true);
    try {
      const project = await createCurrentLocalWorkspaceProject({
        localPath,
        name,
        retryChannel,
        workspaceMode: cowork ? "folder" : "git",
      });
      if (cowork) {
        const repository = project.repositories[0];
        if (repository?.repoAddress) {
          await initCoworkHistory({
            repoAddress: repository.repoAddress,
            folder: localPath,
          });
        }
      }
      queryClient.setQueryData<Project[]>(projectsQueryKey, (current = []) => [
        project,
        ...current.filter((item) => item.id !== project.id),
      ]);
      void queryClient.invalidateQueries({ queryKey: projectsQueryKey });
      setLocalPath(null);
      setName("");
      setCowork(false);
      setRetryChannel(null);
      toast.success(
        cowork
          ? `Cowork Project "${project.name}" added.`
          : `Repository "${project.name}" added from local folder.`,
      );
    } catch (error) {
      if (error instanceof ProjectLocalWorkspaceCreateError) {
        setRetryChannel(error.retryChannel);
      }
      toast.error(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      {children(() => void chooseFolder())}
      <CrewAddProjectDialog
        cowork={cowork}
        localPath={localPath}
        name={name}
        onConfirm={() => void confirm()}
        onNameChange={setName}
        onOpenChange={(open) => !open && close()}
        onRetryRelay={() => void relayUrlQuery.refetch()}
        relayError={relayUrlQuery.isError}
        relayPending={relayUrlQuery.isPending || relayUrlQuery.isFetching}
        relayUrl={relayUrlQuery.data ?? null}
        saving={saving}
      />
    </>
  );
}
