import * as React from "react";
import { Menu } from "lucide-react";

import type { WikiToc } from "@/features/wiki/lib/wikiEvents";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/** Collapsed wiki TOC (#205): hamburger over content below 520px container. */
export function WikiTocMenu({
  activeSlug,
  onSelect,
  toc,
}: {
  activeSlug: string;
  onSelect: (slug: string) => void;
  toc: WikiToc | null;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label="Wiki contents"
          className="shrink-0 [@container(min-width:32.5rem)]:hidden"
          data-testid="wiki-toc-menu"
          size="icon"
          type="button"
          variant="ghost"
        >
          <Menu className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="max-h-80 min-w-56">
        {(toc?.sections ?? []).map((section) => (
          <React.Fragment key={section.id}>
            <DropdownMenuLabel className="text-2xs uppercase">
              {section.title}
            </DropdownMenuLabel>
            {section.pages.map((page) => (
              <DropdownMenuItem
                key={page.slug}
                onSelect={() => onSelect(page.slug)}
              >
                <span
                  className={
                    page.slug === activeSlug
                      ? "min-w-0 truncate font-medium"
                      : "min-w-0 truncate text-muted-foreground"
                  }
                >
                  {page.title}
                </span>
              </DropdownMenuItem>
            ))}
          </React.Fragment>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
