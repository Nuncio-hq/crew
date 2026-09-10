import type { RelayEvent } from "@/shared/api/types";
import {
  exactTag,
  HEX64,
  metadata,
  REVISION,
  SLUG,
  tupleDigest,
  UUID4,
  validRepoD,
  validSourceRefs,
  verifiedEvent,
} from "./wikiSnapshotV1Validation";

/** A signed citation binding complete source bytes and an inclusive line range. */
export type WikiSourceRef = [string, string, number, number, number];
/** Ordered section identity, display title and logical page membership. */
export type WikiSnapshotSection = [string, string, string[]];
/** Immutable signed page membership in a verified manifest. */
export type WikiSnapshotPageRef = [
  string,
  string,
  string,
  string,
  string,
  string,
  string,
  WikiSourceRef[],
];
/** Canonical v1 immutable manifest content. */
export type WikiSnapshotManifest = [
  1,
  string,
  string,
  string,
  string,
  string | null,
  WikiSnapshotSection[],
  WikiSnapshotPageRef[],
];
/** Display validation only; native source access independently checks authorization. */
export type VerifiedWikiSnapshot = {
  head: RelayEvent;
  manifestEvent: RelayEvent;
  manifest: WikiSnapshotManifest;
  pages: RelayEvent[];
};

/** Verify complete signed membership before replacing the displayed revision. */
export function verifyWikiSnapshotV1(input: {
  owner: string;
  repoD: string;
  head: unknown;
  manifest: unknown;
  pages: unknown[];
}): VerifiedWikiSnapshot | null {
  try {
    return verifySnapshot(input);
  } catch {
    // Invalid relay data never becomes an empty successful revision.
    return null;
  }
}

/** Verified signed index only; callers must still fetch and verify every page. */
export type VerifiedWikiSnapshotIndex = Omit<VerifiedWikiSnapshot, "pages">;

/** Authenticate the head and canonical manifest before deriving page queries. */
export function verifyWikiSnapshotIndexV1(input: {
  owner: string;
  repoD: string;
  head: unknown;
  manifest: unknown;
}): VerifiedWikiSnapshotIndex | null {
  try {
    return verifyIndex(input);
  } catch {
    return null;
  }
}

function verifyIndex(
  input: Parameters<typeof verifyWikiSnapshotIndexV1>[0],
): VerifiedWikiSnapshotIndex | null {
  if (!HEX64.test(input.owner) || !validRepoD(input.repoD)) return null;
  const head = verifiedEvent(input.head);
  const manifestEvent = verifiedEvent(input.manifest);
  if (!head || !manifestEvent) return null;
  const manifest: unknown = JSON.parse(manifestEvent.content);
  if (
    !validManifest(manifest) ||
    JSON.stringify(manifest) !== manifestEvent.content
  )
    return null;
  const [, , owner, repoD, , , sections, refs] = manifest;
  if (owner !== input.owner || repoD !== input.repoD) return null;
  const manifestHash = tupleDigest(manifest);
  if (
    !commonTags(head, "_toc", manifest) ||
    !commonTags(manifestEvent, `m1-${manifestHash}`, manifest)
  )
    return null;
  const reference = exactTag(head, "wiki-manifest", 3);
  if (
    !reference ||
    reference[1] !== manifestEvent.id ||
    reference[2] !== manifestHash
  )
    return null;
  const expected = exactTag(head, "expected-revision");
  if (!expected || (expected[1] !== "absent" && !HEX64.test(expected[1])))
    return null;
  const bySlug = new Map(refs.map((ref) => [ref[0], ref]));
  const projection = {
    sections: sections.map(([id, title, slugs]) => ({
      id,
      title,
      pages: slugs.map((slug) => {
        const page = bySlug.get(slug);
        if (!page) throw new Error("missing page membership");
        return { slug: page[1], title: page[4] };
      }),
    })),
  };
  if (JSON.stringify(projection) !== head.content) return null;
  return { head, manifestEvent, manifest };
}

function verifySnapshot(
  input: Parameters<typeof verifyWikiSnapshotV1>[0],
): VerifiedWikiSnapshot | null {
  const index = verifyWikiSnapshotIndexV1(input);
  if (!index || input.pages.length > 256) return null;
  const { head, manifestEvent, manifest } = index;
  const [, snapshot, owner, repoD, revision, , , refs] = manifest;
  if (input.pages.length !== refs.length) return null;
  const byId = new Map<string, RelayEvent>();
  for (const raw of input.pages) {
    const event = verifiedEvent(raw);
    if (!event || byId.has(event.id)) return null;
    byId.set(event.id, event);
  }
  const pages: RelayEvent[] = [];
  for (const ref of refs) {
    const [
      logical,
      encoded,
      eventId,
      digest,
      title,
      section,
      language,
      sourceRefs,
    ] = ref;
    const page = byId.get(eventId);
    if (!page || !commonTags(page, encoded, manifest)) return null;
    const envelope = [
      1,
      snapshot,
      owner,
      repoD,
      revision,
      logical,
      title,
      section,
      language,
      sourceRefs,
      page.content,
    ];
    if (tupleDigest(envelope) !== digest || encoded !== `p1-${digest}`)
      return null;
    for (const [tag, value] of [
      ["wiki-slug", logical],
      ["title", title],
      ["section", section],
      ["language", language],
      ["wiki-source-files", JSON.stringify(sourceRefs)],
    ]) {
      if (exactTag(page, tag)?.[1] !== value) return null;
    }
    const sources = page.tags.filter((tag) => tag[0] === "source");
    const projectedSources = [...new Set(sourceRefs.map((ref) => ref[0]))].map(
      (path) => ["source", path],
    );
    if (JSON.stringify(sources) !== JSON.stringify(projectedSources))
      return null;
    pages.push(page);
  }
  return { head, manifestEvent, manifest, pages };
}

function commonTags(
  event: RelayEvent,
  slug: string,
  manifest: WikiSnapshotManifest,
): boolean {
  const [, snapshot, owner, repoD, revision] = manifest;
  if (event.pubkey !== owner) return false;
  const folder = revision.startsWith("folder:");
  const values = [
    ["d", `${repoD}/${slug}`],
    ["a", `30617:${owner}:${repoD}`],
    ["wiki-version", "1"],
    ["wiki-snapshot", snapshot],
    ["source-kind", folder ? "folder" : "git"],
    ["commit", folder ? revision : revision.slice(4)],
  ];
  if (folder && event.tags.some((tag) => tag[0] === "branch")) return false;
  return values.every(([name, value]) => exactTag(event, name)?.[1] === value);
}

function validManifest(value: unknown): value is WikiSnapshotManifest {
  if (!Array.isArray(value) || value.length !== 8 || value[0] !== 1)
    return false;
  const [, snapshot, owner, repoD, revision, previous, sections, pages] = value;
  if (
    typeof snapshot !== "string" ||
    !UUID4.test(snapshot) ||
    typeof owner !== "string" ||
    !HEX64.test(owner) ||
    typeof repoD !== "string" ||
    !validRepoD(repoD) ||
    typeof revision !== "string" ||
    !REVISION.test(revision) ||
    (previous !== null &&
      (typeof previous !== "string" || !HEX64.test(previous))) ||
    !Array.isArray(sections) ||
    !Array.isArray(pages) ||
    pages.length === 0 ||
    pages.length > 256
  )
    return false;
  const traversal: string[] = [];
  const sectionBySlug = new Map<string, string>();
  const sectionIds = new Set<string>();
  for (const section of sections) {
    if (
      !Array.isArray(section) ||
      section.length !== 3 ||
      !metadata(section[0]) ||
      !metadata(section[1]) ||
      !Array.isArray(section[2]) ||
      sectionIds.has(section[0])
    )
      return false;
    sectionIds.add(section[0]);
    for (const slug of section[2]) {
      if (
        typeof slug !== "string" ||
        !SLUG.test(slug) ||
        sectionBySlug.has(slug)
      )
        return false;
      sectionBySlug.set(slug, section[0]);
      traversal.push(slug);
    }
  }
  if (traversal.length !== pages.length) return false;
  const ids = new Set<string>();
  for (let i = 0; i < pages.length; i += 1) {
    const ref = pages[i];
    if (!Array.isArray(ref) || ref.length !== 8) return false;
    const [logical, encoded, id, digest, title, section, language, sources] =
      ref;
    if (
      logical !== traversal[i] ||
      typeof digest !== "string" ||
      !HEX64.test(digest) ||
      encoded !== `p1-${digest}` ||
      typeof id !== "string" ||
      !HEX64.test(id) ||
      ids.has(id) ||
      !metadata(title) ||
      !metadata(language) ||
      sectionBySlug.get(logical) !== section ||
      !validSourceRefs(sources)
    )
      return false;
    ids.add(id);
  }
  return true;
}
