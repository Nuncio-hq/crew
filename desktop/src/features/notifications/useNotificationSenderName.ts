import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { useCommunities } from "@/features/communities/useCommunities";
import {
  usersBatchEntryKey,
  type UsersBatchEntry,
} from "@/features/profile/hooks";
import {
  readCachedUserLabels,
  writeCachedUserLabels,
} from "@/features/profile/lib/userLabelStorage";
import { getUsersBatch } from "@/shared/api/tauriProfiles";
import { senderNameFromSummary } from "./lib/senderName";

export function useNotificationSenderName(): (
  pubkey: string | undefined,
) => string | null {
  const queryClient = useQueryClient();
  const { activeCommunity } = useCommunities();
  const relayUrl = activeCommunity?.relayUrl ?? "";

  return React.useCallback(
    (pubkey) => {
      const normalized = pubkey?.trim().toLowerCase() ?? "";
      if (!normalized) return null;

      const entry = queryClient.getQueryData<UsersBatchEntry>(
        usersBatchEntryKey(normalized),
      );
      if (entry) return senderNameFromSummary(entry.summary);

      const cached = relayUrl
        ? readCachedUserLabels(relayUrl, [normalized])
        : undefined;
      const cachedSummary = cached?.profiles[normalized];
      if (cachedSummary) return senderNameFromSummary(cachedSummary);

      void getUsersBatch([normalized])
        .then((fresh) => {
          queryClient.setQueryData<UsersBatchEntry>(
            usersBatchEntryKey(normalized),
            {
              summary: fresh.profiles[normalized] ?? null,
              fetchedAt: Date.now(),
            },
          );
          if (relayUrl) {
            writeCachedUserLabels(relayUrl, fresh.profiles, fresh.missing);
          }
        })
        .catch(() => {});
      return null;
    },
    [queryClient, relayUrl],
  );
}
