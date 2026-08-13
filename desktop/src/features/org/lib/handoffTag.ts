import { CREW_HANDOFF_TAG, normalizeOrgPubkey } from "./orgRoster.ts";

export type CrewHandoff = {
  executor: string;
  goalDigest: string;
};

export function parseHandoffTag(
  tags: readonly (readonly string[])[] | undefined,
): CrewHandoff | null {
  const tag = tags?.find((entry) => entry[0] === CREW_HANDOFF_TAG);
  const executor = tag?.[1] ? normalizeOrgPubkey(tag[1]) : null;
  const digest = tag?.[2]?.trim().toLowerCase() ?? "";
  if (!executor || digest.length !== 64 || !/^[0-9a-f]+$/.test(digest)) {
    return null;
  }
  return { executor, goalDigest: digest };
}

export function parentEventIdFromTags(
  tags: readonly (readonly string[])[] | undefined,
): string | null {
  const tagged = tags?.find(
    (entry) =>
      entry[0] === "e" &&
      typeof entry[1] === "string" &&
      /^[0-9a-f]{64}$/i.test(entry[1]),
  );
  return tagged?.[1]?.toLowerCase() ?? null;
}
