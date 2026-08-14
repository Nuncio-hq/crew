import { PaneEmptyState } from "@/shared/ui/PaneEmptyState";

/** Thread-panel empty replies (#205 P6). Extracted so MessageThreadPanel stays under D-022. */
export function ThreadPanelEmptyState() {
  return (
    <PaneEmptyState
      description="Reply in the thread to continue this branch."
      narrowTitle="No replies yet"
      testId="thread-empty-state"
      title="No replies in this branch yet"
    />
  );
}
