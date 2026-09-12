import * as React from "react";

import type { RelayEvent } from "@/shared/api/types";
import {
  parseEntityLink,
  type ParsedEntityLink,
} from "@/shared/lib/entityLink";
import { useOpenEntityLink } from "@/shared/ui/markdown/entityLinks";
import { Markdown } from "@/shared/ui/markdown";
import {
  findWikiSourceReference,
  type WikiSourceOpenRequest,
} from "@/features/wiki/lib/wikiSourceReferences";

export function WikiMarkdown({
  source,
  owner,
  repoD,
  pageEvent,
  onOpenSource,
  onSourceUnavailable,
}: {
  source: string;
  owner: string;
  repoD: string;
  pageEvent: RelayEvent;
  onOpenSource?: (
    request: Required<WikiSourceOpenRequest>,
    trigger?: HTMLElement | null,
  ) => void;
  onSourceUnavailable?: (message: string) => void;
}) {
  const openEntityLink = useOpenEntityLink();
  const rewritten = React.useMemo(
    () => rewriteRelativeFileLinks(source, owner, repoD),
    [owner, repoD, source],
  );
  const handleEntityLink = React.useCallback(
    (link: ParsedEntityLink, trigger?: HTMLElement | null) => {
      if (link.type !== "file") {
        openEntityLink(link);
        return;
      }

      const currentOwner = owner.toLowerCase();
      const coordinateMatches =
        link.owner.toLowerCase() === currentOwner && link.dtag === repoD;
      const match = coordinateMatches
        ? findWikiSourceReference(pageEvent, {
            path: link.path,
            startLine: link.startLine,
            endLine: link.endLine,
          })
        : null;
      if (!coordinateMatches || !match) {
        onSourceUnavailable?.(
          "This source citation is unavailable for the published page revision.",
        );
        return;
      }

      onOpenSource?.(
        {
          path: link.path,
          startLine: match.reference[3],
          endLine: match.reference[4],
        },
        trigger,
      );
    },
    [
      onOpenSource,
      onSourceUnavailable,
      openEntityLink,
      owner,
      pageEvent,
      repoD,
    ],
  );
  return (
    <div data-testid="wiki-markdown">
      <Markdown content={rewritten} onOpenEntityLink={handleEntityLink} />
    </div>
  );
}

/**
 * Add the current repository coordinate to relative Wiki citations. A broken
 * percent escape is preserved as authored content so rendering never throws.
 */
export function rewriteRelativeFileLinks(
  source: string,
  owner: string,
  repoD: string,
): string {
  if (owner.length !== 64) return source;
  return source.replace(
    /buzz:\/\/file\?path=([^&\s)]+)(&lines=([^&\s)]+))?/g,
    (match: string, path: string, _linesPart?: string, lines?: string) => {
      try {
        const decodedPath = decodeURIComponent(path);
        const params = new URLSearchParams({
          owner: owner.toLowerCase(),
          d: repoD,
          path: decodedPath,
        });
        if (lines) params.set("lines", lines);
        return `buzz://file?${params.toString()}`;
      } catch {
        return match;
      }
    },
  );
}

/** Parse a canonical file href without allowing malformed authored URLs out. */
export function parseWikiFileHref(href: string): ParsedEntityLink | null {
  const parsed = parseEntityLink(href);
  return parsed.ok && parsed.value.type === "file" ? parsed.value : null;
}
