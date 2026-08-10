import type * as React from "react";
import type { EditAgentFocusTarget } from "@/features/agents/openEditAgentEvent";

export function HermesProfileConfigRepairRow({
  diagnostic,
  onOpenEditAgent,
  profile,
}: {
  diagnostic: string;
  onOpenEditAgent: (
    e: React.MouseEvent,
    focus: EditAgentFocusTarget | undefined,
  ) => void;
  profile: string;
}) {
  return (
    <div className="space-y-1.5 text-xs leading-4 text-muted-foreground">
      <span className="block [overflow-wrap:anywhere]">
        Hermes profile <code>{profile}</code> has invalid config.yaml:{" "}
        {diagnostic}
      </span>
      <button
        className="relative z-20 font-medium hover:underline"
        onClick={(e) =>
          onOpenEditAgent(e, {
            type: "normalized_field",
            field: "hermesProfile",
          })
        }
        type="button"
      >
        Review profile binding →
      </button>
    </div>
  );
}
