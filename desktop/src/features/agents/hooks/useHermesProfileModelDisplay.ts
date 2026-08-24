import { useQuery } from "@tanstack/react-query";

import { readHermesProfileModel } from "@/shared/api/hermesProfiles";
import { crewMayReadHermesProfileModel } from "../lib/hermesProfileBinding";

export const hermesProfileModelQueryKey = (name: string) =>
  ["hermes-profile-model", name] as const;

/**
 * Reads the model id from a Hermes profile config on disk (including home
 * `default` → `~/.hermes/config.yaml`). Shared query key with profile bind UI.
 */
export function useHermesProfileModelDisplay(
  profileName: string | null | undefined,
): string | null {
  const trimmed = profileName?.trim() || "";
  const query = useQuery({
    queryKey: hermesProfileModelQueryKey(trimmed),
    queryFn: () => readHermesProfileModel(trimmed),
    enabled: Boolean(trimmed) && crewMayReadHermesProfileModel(trimmed),
    refetchOnMount: "always",
  });
  if (query.data?.status !== "ok") return null;
  return query.data.model?.trim() || null;
}
