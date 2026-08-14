import { PaneEmptyState } from "@/shared/ui/PaneEmptyState";

export function ChannelScreenEmptyState() {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 items-center justify-center px-6 py-8">
      <PaneEmptyState
        className="border-0 bg-transparent px-0 py-0"
        narrowTitle="Select a channel"
        title="Select a channel to view messages."
      />
    </div>
  );
}
