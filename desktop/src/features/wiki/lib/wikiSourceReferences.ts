import type { RelayEvent } from "@/shared/api/types";

/**
 * Immutable source reference recorded on a published Wiki page.
 *
 * The path and line range are part of the citation identity. The digest and
 * byte count are retained here so callers never have to reinterpret the wire
 * tuple before handing the page back to native verification.
 */
export type WikiSourceReference = readonly [
  path: string,
  digest: string,
  bytes: number,
  startLine: number,
  endLine: number,
];

export type WikiSourceOpenRequest = {
  path: string;
  startLine?: number;
  endLine?: number;
};

/** Parse the canonical source-reference tag without throwing on old pages. */
export function wikiSourceReferences(event: RelayEvent): WikiSourceReference[] {
  const tag = event.tags.find(
    (candidate) => candidate[0] === "wiki-source-files",
  );
  if (!tag?.[1]) return [];
  try {
    const value = JSON.parse(tag[1]) as unknown;
    if (!Array.isArray(value)) return [];
    return value.filter(isWikiSourceReference);
  } catch {
    return [];
  }
}

/** Find one exact citation, including its recorded line range. */
export function findWikiSourceReference(
  event: RelayEvent,
  request: WikiSourceOpenRequest,
): { index: number; reference: WikiSourceReference } | null {
  const references = wikiSourceReferences(event);
  const index = references.findIndex(
    (reference) =>
      reference[0] === request.path &&
      (request.startLine === undefined || reference[3] === request.startLine) &&
      (request.endLine === undefined || reference[4] === request.endLine),
  );
  return index < 0
    ? null
    : { index, reference: references[index] as WikiSourceReference };
}

/** Match only a fully specified citation; missing ranges are not verified. */
export function matchesWikiSourceCitation(
  event: RelayEvent,
  request: Required<WikiSourceOpenRequest>,
): { index: number; reference: WikiSourceReference } | null {
  return findWikiSourceReference(event, request);
}

function isWikiSourceReference(value: unknown): value is WikiSourceReference {
  return (
    Array.isArray(value) &&
    value.length === 5 &&
    typeof value[0] === "string" &&
    typeof value[1] === "string" &&
    typeof value[2] === "number" &&
    Number.isFinite(value[2]) &&
    typeof value[3] === "number" &&
    Number.isInteger(value[3]) &&
    value[3] > 0 &&
    typeof value[4] === "number" &&
    Number.isInteger(value[4]) &&
    value[4] >= value[3]
  );
}
