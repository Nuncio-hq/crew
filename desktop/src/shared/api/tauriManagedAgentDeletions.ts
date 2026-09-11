import { invokeTauri } from "@/shared/api/tauri";

export type ManagedAgentDeletionSummary = {
  id: string;
  owner: string;
  community: string;
  resourceKey: string;
  revision: number;
  status: string;
  reconciled: boolean;
  updatedAt: number;
};

type RawManagedAgentDeletionSummary = {
  id: string;
  owner: string;
  community: string;
  resource_key: string;
  revision: number;
  status: string;
  reconciled: boolean;
  updated_at: number;
};

export async function listManagedAgentDeletions(): Promise<
  ManagedAgentDeletionSummary[]
> {
  return (
    await invokeTauri<RawManagedAgentDeletionSummary[]>(
      "list_managed_agent_deletions",
    )
  ).map((summary) => ({
    id: summary.id,
    owner: summary.owner,
    community: summary.community,
    resourceKey: summary.resource_key,
    revision: summary.revision,
    status: summary.status,
    reconciled: summary.reconciled,
    updatedAt: summary.updated_at,
  }));
}

export async function retryManagedAgentDeletion(
  operationId: string,
): Promise<void> {
  await invokeTauri("retry_managed_agent_delete", { operationId });
}
