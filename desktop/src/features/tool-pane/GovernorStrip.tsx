import * as React from "react";
import { toast } from "sonner";

import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

import { formatBytes, governorStop, simKeep } from "./governorClient";
import { useGovernorStatus } from "./governorStore";

export function GovernorStrip() {
  const status = useGovernorStatus();
  const conflict = status.capConflict;
  const lastToken = React.useRef<string | null>(null);

  React.useEffect(() => {
    if (!conflict) return;
    if (lastToken.current === conflict.keepToken) return;
    lastToken.current = conflict.keepToken;
    const idleMin = Math.max(1, Math.round(conflict.idleMs / 60_000));
    toast(
      `Sim of #${conflict.victimName} shuts down to make room for #${conflict.incomingName} (idle ${idleMin} min)`,
      {
        duration: 12_000,
        action: {
          label: `Keep #${conflict.victimName}`,
          onClick: () => {
            void simKeep(conflict.victimChannelId);
          },
        },
      },
    );
  }, [conflict]);

  return (
    <div
      className="flex shrink-0 items-center gap-2 border-t border-border/60 px-3 py-1.5 text-2xs text-muted-foreground"
      data-testid="governor-strip"
    >
      <span data-testid="governor-strip-sims">{status.bootedCount} sims</span>
      <span>·</span>
      <span data-testid="governor-strip-streams">
        {status.streamCount} stream
      </span>
      <span>·</span>
      <span data-testid="governor-strip-servers">
        {status.serverCount} servers
      </span>
      <span>·</span>
      <span data-testid="governor-strip-disk">
        {formatBytes(status.diskBytes)}
      </span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            className="ml-auto rounded-md px-1.5 py-0.5 hover:bg-muted hover:text-foreground"
            data-testid="governor-strip-menu"
            type="button"
          >
            Holdings
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-64">
          {status.sims.map((sim) => (
            <DropdownMenuItem
              key={sim.channelId}
              data-testid={`governor-stop-sim-${sim.channelId}`}
              onSelect={() => void governorStop("sim", sim.channelId)}
            >
              Shut down {sim.channelName ?? sim.deviceName}
            </DropdownMenuItem>
          ))}
          {status.servers.map((server) => (
            <DropdownMenuItem
              key={server.id}
              data-testid={`governor-stop-server-${server.id}`}
              onSelect={() => void governorStop("server", server.id)}
            >
              Stop {server.subject} :{server.port}
            </DropdownMenuItem>
          ))}
          <DropdownMenuItem
            data-testid="governor-stop-everything"
            onSelect={() => void governorStop("everything", "")}
          >
            Stop everything
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {conflict ? (
        <Button
          className="h-6"
          data-testid="governor-cap-keep"
          onClick={() => void simKeep(conflict.victimChannelId)}
          size="sm"
          type="button"
          variant="outline"
        >
          Keep #{conflict.victimName}
        </Button>
      ) : null}
    </div>
  );
}
