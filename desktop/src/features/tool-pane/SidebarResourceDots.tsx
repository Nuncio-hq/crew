import { useGovernorStatus } from "./governorStore";

export function SidebarResourceDots({ channelId }: { channelId: string }) {
  const status = useGovernorStatus();
  const sim = status.sims.find((entry) => entry.channelId === channelId);
  const server = status.servers.find((entry) => entry.channelId === channelId);
  const simOn = sim?.lifecycle === "booted" || sim?.lifecycle === "mirroring";
  const serverOn = server?.face === "running" || server?.face === "idleStop";
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
