import { useGovernorStatus } from "./governorStore";
import { leaseFor, useAgentControlUi } from "./agentControlStore";

export function SidebarResourceDots({ channelId }: { channelId: string }) {
  const status = useGovernorStatus();
  const control = useAgentControlUi();
  const sim = status.sims.find((entry) => entry.channelId === channelId);
  const server = status.servers.find((entry) => entry.channelId === channelId);
  const simLease = leaseFor(control, channelId, "sim");
  const browserLease = leaseFor(control, channelId, "browser");
  const simOn =
    sim?.lifecycle === "booted" ||
    sim?.lifecycle === "mirroring" ||
    simLease?.state === "agentHeld";
  const serverOn =
    server?.face === "running" ||
    server?.face === "idleStop" ||
    browserLease?.state === "agentHeld";
  if (!simOn && !serverOn) return null;
  return (
    <span
      className="flex shrink-0 items-center gap-px text-3xs leading-none"
      data-testid={`channel-resource-dots-${channelId}`}
    >
      {simOn ? (
        <span data-testid="resource-dot-sim" title="Simulator booted">
          🟢
        </span>
      ) : null}
      {serverOn ? (
        <span data-testid="resource-dot-server" title="Dev server running">
          ▲
        </span>
      ) : null}
    </span>
  );
}
