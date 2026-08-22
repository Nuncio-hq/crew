import type { UserProfileSummary } from "@/shared/api/types";

export function senderNameFromSummary(
  summary: UserProfileSummary | null | undefined,
): string | null {
  const displayName = summary?.displayName?.trim();
  if (displayName) return displayName;

  const nip05Handle = summary?.nip05Handle?.trim();
  if (nip05Handle) return nip05Handle;

  return null;
}
