import * as React from "react";

import { Markdown } from "@/shared/ui/markdown";

export function WikiMarkdown({
  source,
  owner,
  repoD,
}: {
  source: string;
  owner: string;
  repoD: string;
}) {
  const rewritten = React.useMemo(
    () => rewriteRelativeFileLinks(source, owner, repoD),
    [owner, repoD, source],
  );
  return (
    <div data-testid="wiki-markdown">
      <Markdown content={rewritten} />
    </div>
  );
}

function rewriteRelativeFileLinks(
  source: string,
  owner: string,
  repoD: string,
): string {
  if (owner?.length !== 64) return source;
  return source.replace(
    /buzz:\/\/file\?path=([^&\s)]+)(&lines=([^&\s)]+))?/g,
    (_match, path: string, _linesPart: string | undefined, lines?: string) => {
      const params = new URLSearchParams({
        owner,
        d: repoD,
        path: decodeURIComponent(path),
      });
      if (lines) params.set("lines", lines);
      return `buzz://file?${params.toString()}`;
    },
  );
}
