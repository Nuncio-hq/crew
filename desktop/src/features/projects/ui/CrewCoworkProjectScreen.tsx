import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Folder } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { isCoworkProject } from "@/features/projects/lib/cowork-project";
import { selectProjectRepository } from "@/features/projects/projectModels";
import { useProjectQuery } from "@/features/projects/hooks";
import {
  compactCoworkHistory,
  listCoworkVersions,
  restoreCoworkFile,
  restoreCoworkFolder,
  type CoworkVersionEntry,
  type CoworkVersionsSnapshot,
} from "@/shared/api/coworkVersions";
import { Button } from "@/shared/ui/button";

function formatWhen(timestamp: number): string {
  if (!timestamp) return "";
  return new Date(timestamp * 1000).toLocaleString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    month: "short",
    day: "numeric",
  });
}

function megabytes(bytes: number): string {
  return `${Math.round(bytes / (1024 * 1024))} MB`;
}

export function CrewCoworkProjectScreen({
  projectId,
  threadId,
}: {
  projectId: string;
  threadId?: string;
}) {
  const { goProjects, goChannel } = useAppNavigation();
  const queryClient = useQueryClient();
  const projectQuery = useProjectQuery(projectId);
  const project = projectQuery.data;
  const repository = selectProjectRepository(project, undefined);
  const folder = repository?.localWorkspacePath ?? null;
  const repoAddress = repository?.repoAddress ?? null;
  const versionsQuery = useQuery({
    enabled: Boolean(folder && repoAddress),
    queryFn: () =>
      listCoworkVersions({
        folder: folder ?? "",
        repoAddress: repoAddress ?? "",
      }),
    queryKey: ["cowork-versions", repoAddress, folder],
  });
  const snapshot = versionsQuery.data;

  const applySnapshot = React.useCallback(
    (next: CoworkVersionsSnapshot) => {
      queryClient.setQueryData(["cowork-versions", repoAddress, folder], next);
    },
    [folder, queryClient, repoAddress],
  );

  const restoreFile = useMutation({
    mutationFn: (input: { commit: string; relativePath: string }) =>
      restoreCoworkFile({
        commit: input.commit,
        folder: folder ?? "",
        relativePath: input.relativePath,
        repoAddress: repoAddress ?? "",
      }),
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
    onSuccess: applySnapshot,
  });
  const restoreFolder = useMutation({
    mutationFn: (commit: string) =>
      restoreCoworkFolder({
        commit,
        folder: folder ?? "",
        repoAddress: repoAddress ?? "",
      }),
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
    onSuccess: applySnapshot,
  });
  const compact = useMutation({
    mutationFn: () =>
      compactCoworkHistory({
        folder: folder ?? "",
        repoAddress: repoAddress ?? "",
      }),
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
    onSuccess: applySnapshot,
  });

  if (projectQuery.isPending) {
    return <p className="p-6 text-sm text-muted-foreground">Loading…</p>;
  }
  if (!project || !isCoworkProject(project) || !repository || !folder) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 px-4 py-16 text-center">
        <p className="text-sm text-muted-foreground">
          This Cowork Project could not be found.
        </p>
        <Button onClick={() => void goProjects()} size="sm" variant="outline">
          Back to Projects
        </Button>
      </div>
    );
  }

  const versions = (snapshot?.versions ?? []).filter((entry) =>
    threadId ? entry.threadId === threadId : true,
  );
  const channelId = project.projectChannelId ?? repository.channelId ?? null;

  return (
    <section
      className="flex min-h-0 flex-1 flex-col overflow-hidden"
      data-testid="cowork-versions-timeline"
    >
      <header className="flex items-center gap-3 border-b border-border/60 px-5 py-3">
        <Button
          onClick={() => void goProjects()}
          size="sm"
          type="button"
          variant="ghost"
        >
          <ArrowLeft className="mr-1.5 h-4 w-4" />
          Projects
        </Button>
        <Folder className="h-4 w-4 text-muted-foreground" />
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">{project.name}</h1>
          <p className="truncate font-mono text-2xs text-muted-foreground">
            {folder}
          </p>
        </div>
        {channelId ? (
          <Button
            className="ml-auto"
            onClick={() => void goChannel(channelId)}
            size="sm"
            type="button"
            variant="outline"
          >
            Open channel
          </Button>
        ) : null}
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {snapshot?.notice ? (
          <p
            className="mb-4 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive"
            data-testid="cowork-history-notice"
          >
            {snapshot.notice}
          </p>
        ) : null}
        {snapshot?.excluded.length ? (
          <p
            className="mb-4 text-sm text-muted-foreground"
            data-testid="cowork-excluded-notice"
          >
            {snapshot.excluded.length} files not versioned (&gt;
            {megabytes(snapshot.sizeThresholdBytes)})
          </p>
        ) : null}
        {threadId ? (
          <p className="mb-3 text-sm text-muted-foreground">
            Showing versions for this thread.
          </p>
        ) : null}
        <ol className="space-y-3">
          {versions.map((entry) => (
            <VersionRow
              entry={entry}
              key={entry.id}
              onRestoreFile={(path) =>
                restoreFile.mutate({ commit: entry.id, relativePath: path })
              }
              onRestoreFolder={() => restoreFolder.mutate(entry.id)}
            />
          ))}
        </ol>
        {versions.length === 0 ? (
          <p className="text-sm text-muted-foreground">No versions yet.</p>
        ) : null}
        <div className="mt-6">
          <Button
            data-testid="cowork-compact-history"
            disabled={compact.isPending}
            onClick={() => compact.mutate()}
            size="sm"
            type="button"
            variant="outline"
          >
            Compact history
          </Button>
        </div>
      </div>
    </section>
  );
}

function VersionRow({
  entry,
  onRestoreFile,
  onRestoreFolder,
}: {
  entry: CoworkVersionEntry;
  onRestoreFile: (path: string) => void;
  onRestoreFolder: () => void;
}) {
  const when = formatWhen(entry.timestamp);
  const folderLabel =
    entry.kind === "turn" && entry.agentName
      ? `Back to before ${entry.agentName}'s turn${when ? ` (${when})` : ""}`
      : "Restore this version of the folder";
  return (
    <li
      className="rounded-lg border border-border/60 px-3 py-3"
      data-kind={entry.kind}
      data-testid="cowork-version-entry"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-sm font-medium text-foreground">{entry.summary}</p>
          <p className="text-2xs text-muted-foreground">{when}</p>
          {entry.filesChanged.length > 0 ? (
            <ul className="mt-2 space-y-1">
              {entry.filesChanged.map((path) => (
                <li
                  className="flex items-center justify-between gap-2 text-sm"
                  key={path}
                >
                  <span className="truncate font-mono text-2xs">{path}</span>
                  <Button
                    data-testid="cowork-restore-file"
                    onClick={() => onRestoreFile(path)}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    Restore this version
                  </Button>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
        <Button
          data-testid="cowork-restore-folder"
          onClick={onRestoreFolder}
          size="sm"
          type="button"
          variant="outline"
        >
          {folderLabel}
        </Button>
      </div>
    </li>
  );
}
