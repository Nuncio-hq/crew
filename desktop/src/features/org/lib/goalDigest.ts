import { peekPendingHandoff, takePendingHandoff } from "./pendingHandoff.ts";

export async function goalDigest(goal: string): Promise<string> {
  const bytes = new TextEncoder().encode(goal);
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(hash)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export function buildHandoffTag(
  executor: string,
  digest: string,
): [string, string, string] {
  return ["crew-handoff", executor.toLowerCase(), digest.toLowerCase()];
}

export const SUB_KICKOFF_NEEDS_PARENT =
  "Sub-kickoff must reply to the parent thread so the executor can read the original goal.";

export async function consumePendingHandoffTags(
  content: string,
  hasParent = false,
): Promise<{ extraTags: string[][]; executor: string } | null> {
  const pending = peekPendingHandoff();
  if (!pending) {
    return null;
  }
  if (pending.requiresParent && !hasParent) {
    throw new Error(SUB_KICKOFF_NEEDS_PARENT);
  }
  takePendingHandoff();
  const digest = await goalDigest(content);
  return {
    extraTags: [buildHandoffTag(pending.executor, digest)],
    executor: pending.executor,
  };
}
