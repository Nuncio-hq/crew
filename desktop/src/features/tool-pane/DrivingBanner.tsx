import { invokeGovernor } from "./governorStore";

import type { LeaseView } from "./agentControlStore";

export function DrivingBanner({
  lease,
  instrument,
}: {
  lease: LeaseView | null;
  instrument: "browser" | "sim";
}) {
  if (!lease || lease.state === "free") return null;
  const agent = lease.agentName || "Agent";
  const human = lease.state === "humanHeld";
  return (
    <div
      className="flex shrink-0 items-center gap-2 border-l-2 border-primary bg-primary/10 px-3 py-1.5 text-sm"
      data-lease-state={lease.state}
      data-testid={`${instrument}-driving-banner`}
    >
      <span className="min-w-0 flex-1 truncate text-foreground">
        {human
          ? `\u270B You have control \u00b7 ${agent} is waiting`
          : `\u{1F916} ${agent} is driving`}
      </span>
      <button
        className="shrink-0 rounded-md border border-border bg-background px-2 py-0.5 text-2xs text-foreground hover:bg-muted"
        data-testid={
          human
            ? `${instrument}-lease-release`
            : `${instrument}-lease-take-over`
        }
        type="button"
        onPointerDown={(event) => {
          event.stopPropagation();
        }}
        onClick={(event) => {
          event.stopPropagation();
          const command = human
            ? "agent_control_release"
            : "agent_control_take_over";
          void invokeGovernor(command, {
            input: { channelId: lease.channelId, instrument },
          });
        }}
      >
        {human ? "Release" : "Take over"}
      </button>
    </div>
  );
}
