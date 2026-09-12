import * as React from "react";

import { WikiPageView } from "@/features/wiki/ui/WikiPageView";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import { useWikiPublicationRecovery } from "@/features/wiki/hooks/useWikiPublicationRecovery";
import { jobForRepo } from "@/features/wiki/lib/wikiEvents";
import { getWikiJobs, subscribeWikiJobs } from "@/features/wiki/lib/wikiStore";
import { sameOwnerOperationScope } from "@/shared/api/ownerOperations";
import type { Repository } from "@/features/projects/hooks";
import { wikiRepositoryCoordinate } from "@/shared/api/wikiSnapshot";

export function WikiProjectTab({
  project,
  projectId,
}: {
  project: Repository;
  projectId: string;
}) {
  const coordinate = wikiRepositoryCoordinate(project.owner, project.dtag);
  const eventsQuery = useWikiEventsQuery([project], {
    priorityCoordinates: [coordinate],
  });
  // The Library and project tab share one scope-keyed native status query;
  // mounting it here keeps durable recovery visible after a library unmount.
  const operationScope = eventsQuery.scopeQuery.error
    ? undefined
    : eventsQuery.scopeQuery.data;
  const recovery = useWikiPublicationRecovery(operationScope);
  const jobs = React.useSyncExternalStore(
    subscribeWikiJobs,
    getWikiJobs,
    getWikiJobs,
  );
  const job = jobForRepo(jobs, project.owner, project.dtag);
  const recoveryAction =
    job?.operationId &&
    job.operationRevision !== undefined &&
    job.scope &&
    operationScope &&
    sameOwnerOperationScope(job.scope, operationScope)
      ? {
          operationId: job.operationId,
          operationRevision: job.operationRevision,
          expectedScope: job.scope,
        }
      : null;
  const toc =
    eventsQuery.data?.tocs.find(
      (item) => item.owner === project.owner && item.repoD === project.dtag,
    ) ?? null;
  const pages =
    eventsQuery.data?.pages.filter(
      (item) =>
        item.event.pubkey.toLowerCase() === project.owner.toLowerCase() &&
        item.repoD === project.dtag,
    ) ?? [];
  const repoState = eventsQuery.data?.states.find(
    (event) =>
      event.tags.some((tag) => tag[0] === "d" && tag[1] === project.dtag) &&
      (event.pubkey.toLowerCase() === project.owner.toLowerCase() ||
        event.tags.some(
          (tag) => tag[0] === "a" && tag[1] === project.repoAddress,
        )),
  );
  const readStatus = eventsQuery.data?.repositoryStatuses[coordinate];
  const snapshot = eventsQuery.data?.repositorySnapshots[coordinate] ?? null;
  return (
    <WikiPageView
      admin={false}
      askScope="repo"
      channelId={project.channelId ?? null}
      door="project"
      owner={project.owner}
      navigationProjectId={projectId}
      page={pages[0] ?? null}
      pages={pages}
      repoD={project.dtag}
      repoName={project.name}
      repoPath={project.localWorkspacePath}
      workspaceMode={project.workspaceMode}
      repoState={repoState}
      readStatus={readStatus}
      snapshot={snapshot}
      onRetryRead={() => eventsQuery.retryRepository(coordinate)}
      operationScope={operationScope}
      toc={toc}
      recoveryJob={job}
      onRecoveryRetry={
        recoveryAction
          ? () =>
              recovery.recover({
                action: "retry",
                repoKey: `${project.owner.toLowerCase()}:${project.dtag}`,
                ...recoveryAction,
              })
          : undefined
      }
      onRecoveryReconcile={
        recoveryAction
          ? () =>
              recovery.recover({
                action: "reconcile",
                repoKey: `${project.owner.toLowerCase()}:${project.dtag}`,
                ...recoveryAction,
              })
          : undefined
      }
      onRecoveryCancel={
        recoveryAction
          ? () =>
              recovery.recover({
                action: "cancel",
                repoKey: `${project.owner.toLowerCase()}:${project.dtag}`,
                ...recoveryAction,
              })
          : undefined
      }
      onRegenerate={
        recoveryAction && job?.retiredDependencyId && project.localWorkspacePath
          ? () =>
              recovery.recover({
                action: "regenerate",
                repoKey: `${project.owner.toLowerCase()}:${project.dtag}`,
                repoPath: project.localWorkspacePath,
                workspaceMode: project.workspaceMode,
                retiredDependencyId: job.retiredDependencyId,
                ...recoveryAction,
              })
          : undefined
      }
      recoveryPending={recovery.isPending}
      regeneratePending={recovery.isPending}
    />
  );
}
