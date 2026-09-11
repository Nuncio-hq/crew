import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  listManagedAgentDeletions,
  retryManagedAgentDeletion,
} from "@/shared/api/tauriManagedAgentDeletions";
import {
  managedAgentsQueryKey,
  relayAgentsQueryKey,
} from "@/features/agents/hooks";

export const managedAgentDeletionsQueryKey = [
  "managed-agent-deletions",
] as const;

export function useManagedAgentDeletionsQuery() {
  return useQuery({
    queryKey: managedAgentDeletionsQueryKey,
    queryFn: listManagedAgentDeletions,
    staleTime: 30_000,
  });
}

export function useRetryManagedAgentDeletionMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (operationId: string) => retryManagedAgentDeletion(operationId),
    onSettled: async () => {
      await queryClient.invalidateQueries({
        queryKey: managedAgentDeletionsQueryKey,
      });
      await queryClient.invalidateQueries({ queryKey: managedAgentsQueryKey });
      await queryClient.invalidateQueries({ queryKey: relayAgentsQueryKey });
    },
  });
}
