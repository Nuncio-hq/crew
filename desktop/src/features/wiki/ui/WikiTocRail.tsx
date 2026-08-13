import * as React from "react";

import type { WikiToc } from "@/features/wiki/lib/wikiEvents";

export function WikiTocRail({
  toc,
  activeSlug,
  filter,
  onSelect,
}: {
  toc: WikiToc | null;
  activeSlug: string;
  filter?: string;
  onSelect: (slug: string) => void;
}) {
  const needle = (filter ?? "").trim().toLowerCase();
  const items = React.useMemo(
    () =>
      toc?.sections.flatMap((section) =>
        section.pages
          .filter(
            (page) =>
              !needle ||
              page.title.toLowerCase().includes(needle) ||
              page.slug.toLowerCase().includes(needle),
          )
          .map((page) => ({
            section: section.title,
            slug: page.slug,
            title: page.title,
          })),
      ),
    [needle, toc],
  );
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!items?.length) return;
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const idx = items.findIndex((item) => item.slug === activeSlug);
      const next =
        event.key === "ArrowDown"
          ? items[Math.min(items.length - 1, idx + 1)]
          : items[Math.max(0, idx - 1)];
      if (next) onSelect(next.slug);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeSlug, items, onSelect]);

  return (
    <nav
      aria-label="Wiki contents"
      className="w-52 shrink-0 overflow-auto border-r border-border p-3"
      data-testid="wiki-toc"
    >
      {(toc?.sections ?? []).map((section) => (
        <div className="mb-3" key={section.id}>
          <div className="px-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground">
            {section.title}
          </div>
          {section.pages
            .filter(
              (page) =>
                !needle ||
                page.title.toLowerCase().includes(needle) ||
                page.slug.toLowerCase().includes(needle),
            )
            .map((page) => (
              <button
                className={
                  page.slug === activeSlug
                    ? "mt-0.5 block w-full rounded-md bg-muted px-2 py-1 text-left text-sm"
                    : "mt-0.5 block w-full rounded-md px-2 py-1 text-left text-sm text-muted-foreground hover:text-foreground"
                }
                data-testid={`wiki-toc-${page.slug}`}
                key={page.slug}
                onClick={() => onSelect(page.slug)}
                type="button"
              >
                {page.title}
              </button>
            ))}
        </div>
      ))}
    </nav>
  );
}
