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
  const railRef = React.useRef<HTMLElement>(null);
  const onPageKeyDown = (
    event: React.KeyboardEvent<HTMLButtonElement>,
    slug: string,
  ) => {
    if (
      event.defaultPrevented ||
      event.altKey ||
      event.ctrlKey ||
      event.metaKey ||
      event.shiftKey ||
      event.nativeEvent.isComposing ||
      !items?.length ||
      (event.key !== "ArrowDown" && event.key !== "ArrowUp")
    ) {
      return;
    }
    const index = items.findIndex((item) => item.slug === slug);
    if (index < 0) return;
    const next =
      event.key === "ArrowDown"
        ? items[Math.min(items.length - 1, index + 1)]
        : items[Math.max(0, index - 1)];
    if (!next) return;
    event.preventDefault();
    onSelect(next.slug);
    const buttons = railRef.current?.querySelectorAll<HTMLButtonElement>(
      "button[data-wiki-page]",
    );
    Array.from(buttons ?? [])
      .find((button) => button.dataset.wikiPage === next.slug)
      ?.focus();
  };

  return (
    <nav
      aria-label="Wiki contents"
      className="hidden w-52 shrink-0 overflow-auto border-r border-border p-3 [@container(min-width:32.5rem)]:block"
      data-testid="wiki-toc"
      ref={railRef}
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
                    ? "mt-0.5 block w-full truncate rounded-md bg-muted px-2 py-1 text-left text-sm"
                    : "mt-0.5 block w-full truncate rounded-md px-2 py-1 text-left text-sm text-muted-foreground hover:text-foreground"
                }
                data-testid={`wiki-toc-${page.slug}`}
                data-wiki-page={page.slug}
                key={page.slug}
                onClick={() => onSelect(page.slug)}
                onKeyDown={(event) => onPageKeyDown(event, page.slug)}
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
