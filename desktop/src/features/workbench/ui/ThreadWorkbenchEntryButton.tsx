import { Hammer } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

export function ThreadWorkbenchEntryButton({ onOpen }: { onOpen: () => void }) {
  return (
    <Button
      className={cn("h-7 gap-1 px-2 text-xs")}
      data-testid="open-thread-workbench"
      onClick={onOpen}
      size="sm"
      type="button"
      variant="ghost"
    >
      <Hammer className="h-3.5 w-3.5" />
      Open workbench
    </Button>
  );
}
