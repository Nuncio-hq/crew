import type { ThreadBreadcrumb as ThreadBreadcrumbData } from "@/features/messages/lib/threadOrientation";
import { cn } from "@/shared/lib/cn";

type ThreadBreadcrumbProps = {
  breadcrumb: ThreadBreadcrumbData;
  onNavigate: () => void;
};

function Separator() {
  return (
    <span aria-hidden className="shrink-0 text-muted-foreground">
      ›
    </span>
  );
}

/**
 * Clickable orientation trail for the thread panel header.
 * One control for the whole trail — clicking anywhere scrolls the timeline
 * to the top-level anchor (and closes the focus drawer when applicable).
 *
 * Truncation rule: `#channel` and every author stay `shrink-0`; only the
 * terminal snippet may ellipsis.
 */
export function ThreadBreadcrumb({
  breadcrumb,
  onNavigate,
}: ThreadBreadcrumbProps) {
  const { channelName, segments, truncated } = breadcrumb;
  const terminal = segments[segments.length - 1];
  // Authors before the terminal. When truncated the builder kept
  // [first, …last two], so insert a visual ellipsis after the first author.
  const priorAuthors = segments.slice(0, -1);

  return (
    <button
      aria-label={`Go to the original message in #${channelName}`}
      className={cn(
        // No flex-1: the docked header's negative-margin overlap sits over the
        // sticky project-thread status bar. Growing this button to fill the
        // header row steals Workspace clicks from that bar.
        "group flex min-w-0 max-w-full items-center gap-1 text-left text-base font-semibold leading-6 tracking-tight",
        "text-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring",
        "rounded-sm",
      )}
      data-testid="thread-breadcrumb"
      onClick={onNavigate}
      type="button"
    >
      <span className="shrink-0 whitespace-nowrap">#{channelName}</span>
      {priorAuthors.map((segment, index) => (
        <span className="contents" key={segment.message.id}>
          <Separator />
          <span className="shrink-0 whitespace-nowrap">{segment.author}</span>
          {truncated && index === 0 ? (
            <>
              <Separator />
              <span aria-hidden className="shrink-0 text-muted-foreground">
                …
              </span>
            </>
          ) : null}
        </span>
      ))}
      <Separator />
      <span className="shrink-0 whitespace-nowrap">{terminal.author}</span>
      {terminal.snippet ? (
        <span className="min-w-0 truncate text-xs font-normal text-muted-foreground group-hover:text-foreground">
          : &ldquo;{terminal.snippet}&rdquo;
        </span>
      ) : null}
    </button>
  );
}
