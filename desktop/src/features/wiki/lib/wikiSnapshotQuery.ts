import type { RelayEvent } from "@/shared/api/types";
import {
  verifyWikiSnapshotIndexV1,
  verifyWikiSnapshotV1,
  type VerifiedWikiSnapshot,
} from "./wikiSnapshotV1";
import {
  exactTag,
  HEX64,
  validRepoD,
  verifiedEvent,
} from "./wikiSnapshotV1Validation";

/** Every request must be executed by the native captured-origin transport. */
export type WikiScopedFilter = {
  kinds: [30623];
  authors: [string];
  "#a": [string];
  "#d": string[];
  ids?: string[];
  limit: number;
};
/** Native transport owns before-send scope checks; this fence controls continuation. */
export type WikiScopedReader = {
  query: (filter: WikiScopedFilter) => Promise<unknown[]>;
  isCurrent: () => boolean;
};
/** Incomplete reads must preserve the caller's previous verified revision. */
export type WikiSnapshotRead =
  | { status: "absent" }
  | { status: "legacy"; head: RelayEvent }
  | { status: "verified"; snapshot: VerifiedWikiSnapshot }
  | { status: "incomplete"; reason: "unavailable" | "invalid" | "interrupted" };

class InterruptedRead extends Error {}
class InvalidRead extends Error {}

/** Read an exact repository head and only that head's immutable members. */
export async function readWikiSnapshot(
  scope: { owner: string; repoD: string },
  reader: WikiScopedReader,
): Promise<WikiSnapshotRead> {
  if (!HEX64.test(scope.owner) || !validRepoD(scope.repoD))
    return { status: "incomplete", reason: "invalid" };
  const coordinate = `30617:${scope.owner}:${scope.repoD}`;
  const query = async (slugs: string[], ids?: string[], limit = 64) => {
    if (!reader.isCurrent()) throw new InterruptedRead();
    const result = await reader.query({
      kinds: [30623],
      authors: [scope.owner],
      "#a": [coordinate],
      "#d": slugs.map((slug) => `${scope.repoD}/${slug}`),
      ...(ids ? { ids } : {}),
      limit,
    });
    if (!reader.isCurrent()) throw new InterruptedRead();
    if (!Array.isArray(result) || result.length > limit)
      throw new InvalidRead();
    return result;
  };
  try {
    const heads = await query(["_toc"], undefined, 2);
    if (heads.length === 0) return { status: "absent" };
    if (heads.length !== 1) throw new InvalidRead();
    const head = verifiedEvent(heads[0]);
    if (
      !head ||
      head.pubkey !== scope.owner ||
      exactTag(head, "a")?.[1] !== coordinate ||
      exactTag(head, "d")?.[1] !== `${scope.repoD}/_toc`
    )
      throw new InvalidRead();
    const versions = head.tags.filter((tag) => tag[0] === "wiki-version");
    if (versions.length === 0) return { status: "legacy", head };
    if (exactTag(head, "wiki-version")?.[1] !== "1") throw new InvalidRead();
    const reference = exactTag(head, "wiki-manifest", 3);
    if (!reference || !HEX64.test(reference[1]) || !HEX64.test(reference[2]))
      throw new InvalidRead();
    const manifests = await query([`m1-${reference[2]}`], [reference[1]], 1);
    if (manifests.length !== 1) throw new InvalidRead();
    const index = verifyWikiSnapshotIndexV1({
      ...scope,
      head,
      manifest: manifests[0],
    });
    if (!index) throw new InvalidRead();
    const refs = index.manifest[7];
    const pages: unknown[] = [];
    for (let offset = 0; offset < refs.length; offset += 64) {
      const batch = refs.slice(offset, offset + 64);
      const events = await query(
        batch.map((ref) => ref[1]),
        batch.map((ref) => ref[2]),
        batch.length,
      );
      if (events.length !== batch.length) throw new InvalidRead();
      pages.push(...events);
    }
    const snapshot = verifyWikiSnapshotV1({
      ...scope,
      head,
      manifest: index.manifestEvent,
      pages,
    });
    if (!snapshot) throw new InvalidRead();
    if (!reader.isCurrent()) throw new InterruptedRead();
    return { status: "verified", snapshot };
  } catch (error) {
    return {
      status: "incomplete",
      reason:
        error instanceof InterruptedRead || !reader.isCurrent()
          ? "interrupted"
          : error instanceof InvalidRead
            ? "invalid"
            : "unavailable",
    };
  }
}
