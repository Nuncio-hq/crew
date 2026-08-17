import * as React from "react";

import { useNeedsYouItems } from "../hooks/useWorkTreeProjection";
import type { NeedsYouItem } from "../lib/workTreeTypes";
import { NeedsYouSection } from "./NeedsYouSection";

export function WorkTreeSidebarBlock({
  onSelectThread,
}: {
  onSelectThread: (channelId: string, threadRootId: string) => void;
}) {
  const needsYou = useNeedsYouItems();
  const [needsYouOpen, setNeedsYouOpen] = React.useState(false);

  const handleOpenNeedsYou = React.useCallback(
    (item: NeedsYouItem) => {
      onSelectThread(item.channelId, item.threadRootId);
    },
    [onSelectThread],
  );

  return (
    <div data-testid="work-tree-sidebar">
      <NeedsYouSection
        count={needsYou.count}
        grouped={needsYou.grouped}
        onOpenItem={handleOpenNeedsYou}
        onToggle={() => setNeedsYouOpen((open) => !open)}
        open={needsYouOpen}
      />
    </div>
  );
}
