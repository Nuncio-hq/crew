import { buildFileLink } from "@/shared/lib/entityLink";
import { useOpenEntityLink } from "@/shared/ui/markdown/entityLinks";
import { parseEntityLink } from "@/shared/lib/entityLink";

export function WikiSourceFiles({
  files,
  owner,
  repoD,
}: {
  files: string[];
  owner: string;
  repoD: string;
}) {
  const open = useOpenEntityLink();
  if (files.length === 0) return null;
  return (
    <details
      className="mb-4 rounded-md border border-border bg-muted/20 p-3"
      data-testid="wiki-source-files"
    >
      <summary className="cursor-pointer text-sm text-muted-foreground">
        Relevant source files ({files.length})
      </summary>
      <ul className="mt-2 space-y-1">
        {files.map((file) => {
          const href =
            owner.length === 64
              ? buildFileLink({
                  owner,
                  dtag: repoD,
                  path: file,
                  lines: "1-40",
                })
              : null;
          return (
            <li key={file}>
              <button
                className="font-mono text-2xs text-primary"
                onClick={() => {
                  if (!href) return;
                  const parsed = parseEntityLink(href);
                  if (parsed.ok) open(parsed.value);
                }}
                type="button"
              >
                {file}
              </button>
            </li>
          );
        })}
      </ul>
    </details>
  );
}
