import type { ThreadBreadcrumbSegment } from "@/features/messages/lib/threadOrientation";
import type { TimelineMessage } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import { UserAvatar } from "@/shared/ui/UserAvatar";

const MAX_VISIBLE_ROWS = 2;

type ThreadAncestryStripProps = {
  /** Ancestors above the head, top-level first. Empty → render nothing. */
  segments: readonly ThreadBreadcrumbSegment[];
  truncated: boolean;
  onOpenThread: (message: TimelineMessage) => void;
};

/**
 * Collapsed ancestor line(s) above a nested thread head. Clicking a row opens
 * that ancestor as the new thread head. Caps at two rows; deeper history is
 * summarized as "N earlier messages" pointing at the top-level ancestor.
 */
export function ThreadAncestryStrip({
  segments,
  truncated,
  onOpenThread,
}: ThreadAncestryStripProps) {
  if (segments.length === 0) {
    return null;
  }

  // Cap at 2 rows total. When there are more ancestors than that (or the
  // breadcrumb already dropped middle segments), collapse the older ones into
  // a single "N earlier messages" row that opens the top-level ancestor, and
  // keep the most recent ancestor as the second row.
  const collapseOlder = truncated || segments.length > MAX_VISIBLE_ROWS;
  const topLevel = segments[0];
  const rows: Array<{
    key: string;
    label: string;
    snippet: string | null;
    author: string | null;
    avatarUrl: string | null;
    message: TimelineMessage;
  }> = [];

  if (collapseOlder) {
    const hiddenCount = Math.max(1, segments.length - 1);
    rows.push({
      key: "earlier",
      label: `${hiddenCount} earlier message${hiddenCount === 1 ? "" : "s"}`,
      snippet: null,
      author: null,
      avatarUrl: null,
      message: topLevel.message,
    });
    const immediate = segments[segments.length - 1];
    if (immediate.message.id !== topLevel.message.id) {
      rows.push({
        key: immediate.message.id,
        label: immediate.author,
        snippet: immediate.snippet,
        author: immediate.author,
        avatarUrl: immediate.message.avatarUrl ?? null,
        message: immediate.message,
      });
    }
  } else {
    for (const segment of segments) {
      rows.push({
        key: segment.message.id,
        label: segment.author,
        snippet: segment.snippet,
        author: segment.author,
        avatarUrl: segment.message.avatarUrl ?? null,
        message: segment.message,
      });
    }
  }

  return (
    <div
      className="flex flex-col gap-1 border-l border-border/45 pl-3"
      data-testid="thread-ancestry-strip"
    >
      {rows.map((row) => (
        <button
          className={cn(
            "flex min-w-0 items-center gap-2 rounded-sm py-0.5 text-left text-xs text-muted-foreground",
            "hover:text-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring",
          )}
          data-testid="thread-ancestry-row"
          key={row.key}
          onClick={() => onOpenThread(row.message)}
          type="button"
        >
          {row.author ? (
            <>
              <UserAvatar
                avatarUrl={row.avatarUrl}
                className="h-5 w-5 shrink-0 text-2xs"
                displayName={row.author}
                size="sm"
              />
              <span className="shrink-0 font-medium">{row.author}</span>
              {row.snippet ? (
                <span className="min-w-0 truncate">
                  &ldquo;{row.snippet}&rdquo;
                </span>
              ) : null}
            </>
          ) : (
            <span className="truncate">{row.label}</span>
          )}
        </button>
      ))}
    </div>
  );
}
