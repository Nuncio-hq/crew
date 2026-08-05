/**
 * Hermes profile cleanup after agent/persona delete (C-13 / C-14).
 */
import {
  deleteHermesProfile,
  hermesProfileLifecycleMessage,
  hermesProfileLifecycleSuccess,
} from "@/shared/api/hermesProfiles";

/** Delete named Hermes profiles; returns an error message if any fail. */
export async function deleteHermesProfilesAfterAgentRemoval(
  profiles: readonly string[],
): Promise<string | null> {
  const unique = [...new Set(profiles.map((p) => p.trim()).filter(Boolean))];
  for (const profile of unique) {
    const result = await deleteHermesProfile(profile);
    if (!hermesProfileLifecycleSuccess(result)) {
      return hermesProfileLifecycleMessage(result);
    }
  }
  return null;
}
