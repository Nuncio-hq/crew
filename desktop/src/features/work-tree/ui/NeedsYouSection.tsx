import {
  NEEDS_YOU_KIND_ORDER,
  needsYouKindHeading,
  needsYouSectionLabel,
} from "../lib/needsYouAggregation";
import type { NeedsYouItem } from "../lib/workTreeTypes";

export function NeedsYouSection({
  count,
  grouped,
  onOpenItem,
  open,
  onToggle,
}: {
  count: number;
  grouped: Record<NeedsYouItem["kind"], NeedsYouItem[]>;
  onOpenItem: (item: NeedsYouItem) => void;
  onToggle: () => void;
  open: boolean;
}) {
  if (count === 0) return null;
  return (
    <div className="px-1 pb-1" data-testid="needs-you-section">
      <button
        className="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-sm hover:bg-sidebar-accent"
        data-testid="needs-you-header"
        onClick={onToggle}
        type="button"
      >
        <span aria-hidden>⚡</span>
        <span className="min-w-0 flex-1 truncate font-medium">
          {needsYouSectionLabel(count)}
        </span>
      </button>
      {open ? (
        <div
          className="mt-1 flex flex-col gap-2 px-1 pb-1"
          data-testid="needs-you-panel"
        >
          {NEEDS_YOU_KIND_ORDER.map((kind) => {
            const items = grouped[kind];
            if (items.length === 0) return null;
            return (
              <div key={kind}>
                <h3 className="px-1 pb-0.5 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
                  {needsYouKindHeading(kind)}
                </h3>
                {items.map((item) => (
                  <button
                    className="flex w-full min-w-0 flex-col rounded-md px-2 py-1 text-left hover:bg-sidebar-accent"
                    data-testid={`needs-you-item-${item.id}`}
                    key={item.id}
                    onClick={() => onOpenItem(item)}
                    type="button"
                  >
                    <span className="truncate text-sm">{item.title}</span>
                  </button>
                ))}
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
