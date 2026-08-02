/**
 * Machine-readable ACP failure notices (kind 9 + tags).
 * Desktop must key Retry off tags — never string-match the body.
 */

export const FAILURE_NOTICE_TAG = "failure_notice";
export const FAILURE_NOTICE_E_MARKER = "failed";

export type FailureNoticeCause =
  | "retry_exhausted"
  | "auth"
  | "panic"
  | "timeout"
  | string;

export type FailureNotice = {
  cause: FailureNoticeCause;
  /** Event ids tagged `e`/`failed` — the only valid Retry targets. */
  failedEventIds: string[];
};

export function parseFailureNotice(
  tags: readonly (readonly string[])[] | null | undefined,
): FailureNotice | null {
  if (!tags || tags.length === 0) return null;

  let cause: FailureNoticeCause | null = null;
  const failedEventIds: string[] = [];
  const seen = new Set<string>();

  for (const tag of tags) {
    if (tag[0] === FAILURE_NOTICE_TAG && typeof tag[1] === "string") {
      cause = tag[1];
      continue;
    }
    if (
      tag[0] === "e" &&
      typeof tag[1] === "string" &&
      tag[3] === FAILURE_NOTICE_E_MARKER &&
      /^[0-9a-f]{64}$/i.test(tag[1])
    ) {
      const id = tag[1].toLowerCase();
      if (!seen.has(id)) {
        seen.add(id);
        failedEventIds.push(id);
      }
    }
  }

  if (cause == null) return null;
  return { cause, failedEventIds };
}
