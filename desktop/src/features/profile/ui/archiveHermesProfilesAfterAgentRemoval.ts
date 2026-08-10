import {
  archiveHermesProfile,
  hermesProfileArchiveMessage,
  hermesProfileArchiveSuccess,
} from "@/shared/api/hermesProfiles";

export async function archiveHermesProfilesAfterAgentRemoval(
  profiles: readonly string[],
): Promise<string | null> {
  const unique = [...new Set(profiles.map((p) => p.trim()).filter(Boolean))];
  for (const profile of unique) {
    const result = await archiveHermesProfile(profile);
    if (!hermesProfileArchiveSuccess(result)) {
      return hermesProfileArchiveMessage(result);
    }
  }
  return null;
}
