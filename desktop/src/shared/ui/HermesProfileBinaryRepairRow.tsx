import type * as React from "react";

export function HermesProfileBinaryRepairRow({
  command,
  onOpenRuntimes,
}: {
  command: string;
  onOpenRuntimes: (e: React.MouseEvent) => void;
}) {
  return (
    <div className="space-y-1.5 text-xs leading-4 text-muted-foreground">
      <span className="block [overflow-wrap:anywhere]">
        Hermes command <code>{command}</code> is missing or not on PATH.
      </span>
      <button
        className="relative z-20 font-medium hover:underline"
        onClick={onOpenRuntimes}
        type="button"
      >
        Install Hermes or fix PATH →
      </button>
    </div>
  );
}
