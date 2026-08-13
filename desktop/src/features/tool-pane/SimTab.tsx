import {
  Camera,
  Circle,
  Home,
  MoreHorizontal,
  RotateCw,
  Settings2,
} from "lucide-react";
import * as React from "react";

import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import {
  formatBytes,
  formatCountdown,
  holdingForChannel,
  simBoot,
  simDelete,
  simErase,
  simEnsureDevice,
  simKeep,
  simSetPaneVisible,
} from "./governorClient";
import { invokeGovernor, useGovernorStatus } from "./governorStore";
import { captureSimPng, postCaptureEvidence } from "./postEvidenceCapture";
import type { CanvasTooling, SimHolding, SimLifecycle } from "./types";

const ENTER_EASE = [0.32, 0.72, 0, 1] as const;

export function SimTab({
  channelId,
  channelName,
  threadRootId,
  tooling,
}: {
  channelId: string;
  channelName: string;
  threadRootId?: string | null;
  tooling: CanvasTooling | null;
}) {
  const status = useGovernorStatus();
  const holding = holdingForChannel(status, channelId);
  const bridge = status.bridge;
  const face: SimLifecycle | "bridge-missing" =
    bridge.availability === "missing" || bridge.availability === "failed"
      ? "bridge-missing"
      : (holding?.lifecycle ?? "absent");

  React.useEffect(() => {
    void simEnsureDevice({
      channelId,
      channelName,
      deviceType: tooling?.simulator?.deviceType,
      runtime: tooling?.simulator?.runtime,
    }).catch(() => undefined);
  }, [
    channelId,
    channelName,
    tooling?.simulator?.deviceType,
    tooling?.simulator?.runtime,
  ]);

  React.useEffect(() => {
    if (face === "bridge-missing" || face === "absent") return;
    void simSetPaneVisible(channelId, true).catch(() => undefined);
    return () => {
      void simSetPaneVisible(channelId, false).catch(() => undefined);
    };
  }, [channelId, face]);

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-testid="tool-pane-sim"
      data-sim-face={face}
    >
      <SimStatusLine holding={holding} onOpenSettings={() => undefined} />
      <div className="flex min-h-0 flex-1 items-center justify-center p-4">
        {face === "bridge-missing" ? (
          <BridgeMissingCard
            hint={bridge.installHint}
            message={bridge.message}
          />
        ) : face === "absent" ? (
          <CreateDeviceCard
            channelId={channelId}
            channelName={channelName}
            tooling={tooling}
          />
        ) : face === "shutdown" ? (
          <ShutdownFace
            holding={holding}
            channelId={channelId}
            channelName={channelName}
          />
        ) : face === "booting" ? (
          <BootingFace holding={holding} />
        ) : face === "booted" || face === "mirroring" ? (
          <MirrorFace
            channelId={channelId}
            holding={holding}
            threadRootId={threadRootId}
          />
        ) : (
          <BootingFace holding={holding} />
        )}
      </div>
      {holding?.idleDeadlineMs != null ? (
        <IdleStrip
          channelId={channelId}
          deadlineMs={holding.idleDeadlineMs}
          nowMs={status.nowMs}
        />
      ) : null}
    </div>
  );
}

function SimStatusLine({
  holding,
  onOpenSettings,
}: {
  holding: SimHolding | undefined;
  onOpenSettings: () => void;
}) {
  const booted =
    holding?.lifecycle === "booted" || holding?.lifecycle === "mirroring";
  return (
    <div
      className="flex shrink-0 items-center gap-2 border-b border-border/60 px-3 py-1.5 text-sm"
      data-testid="sim-status-line"
    >
      <Circle
        className={cn(
          "h-2 w-2 fill-current",
          booted ? "text-emerald-500" : "text-muted-foreground",
        )}
      />
      <span className="text-foreground">
        {booted
          ? "Booted"
          : holding?.lifecycle === "shutdown"
            ? "Shutdown"
            : "No device"}
      </span>
      <span className="text-muted-foreground">·</span>
      <span className="text-muted-foreground">
        {holding?.deviceType ?? "iPhone 16 Pro"}
      </span>
      <span className="text-muted-foreground">·</span>
      <span className="text-muted-foreground">
        {holding?.runtime ?? "iOS 18"}
      </span>
      <button
        className="ml-auto text-muted-foreground hover:text-foreground"
        data-testid="sim-settings"
        onClick={onOpenSettings}
        type="button"
      >
        <Settings2 className="h-4 w-4" />
      </button>
    </div>
  );
}

function CreateDeviceCard({
  channelId,
  channelName,
  tooling,
}: {
  channelId: string;
  channelName: string;
  tooling: CanvasTooling | null;
}) {
  const deviceType = tooling?.simulator?.deviceType ?? "iPhone 16 Pro";
  const runtime = tooling?.simulator?.runtime ?? "iOS 18";
  return (
    <div
      className="max-w-sm rounded-xl border border-border/60 bg-muted/30 p-4 text-center"
      data-testid="sim-face-absent"
    >
      <p className="text-sm text-foreground">
        No simulator for this channel yet.
      </p>
      <p className="mt-1 text-2xs text-muted-foreground">
        Devices are created lazily. Nothing boots until you ask.
      </p>
      <Button
        className="mt-3"
        data-testid="sim-create"
        onClick={() => {
          void simBoot({ channelId, channelName, deviceType, runtime }).catch(
            () => undefined,
          );
        }}
        type="button"
      >
        Create {deviceType} ({runtime})
      </Button>
    </div>
  );
}

function ShutdownFace({
  holding,
  channelId,
  channelName,
}: {
  holding: SimHolding | undefined;
  channelId: string;
  channelName: string;
}) {
  return (
    <div
      className="flex flex-col items-center gap-3"
      data-testid="sim-face-shutdown"
    >
      <div
        className="h-80 w-40 rounded-[2rem] border-4 border-foreground/80 bg-zinc-900 grayscale"
        style={{
          backgroundImage: holding?.lastScreenshotDataUrl
            ? `url(${holding.lastScreenshotDataUrl})`
            : undefined,
          backgroundSize: "cover",
        }}
      />
      <Button
        data-testid="sim-boot"
        onClick={() => {
          void simBoot({ channelId, channelName }).catch(() => undefined);
        }}
        type="button"
      >
        ▶ Boot (~15s)
      </Button>
    </div>
  );
}

function BootingFace({ holding }: { holding: SimHolding | undefined }) {
  const elapsed = Math.round((holding?.bootElapsedMs ?? 0) / 1000);
  return (
    <div
      className="flex h-80 w-40 items-end justify-center rounded-[2rem] border-4 border-foreground/80 bg-zinc-900"
      data-testid="sim-face-booting"
    >
      <p className="mb-6 text-2xs text-zinc-300">Booting… {elapsed}s</p>
    </div>
  );
}

function MirrorFace({
  channelId,
  holding,
  threadRootId,
}: {
  channelId: string;
  holding: SimHolding | undefined;
  threadRootId?: string | null;
}) {
  const udid = holding?.udid;
  const [streamUrl, setStreamUrl] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!udid) return;
    void invokeGovernor<string>("sim_mjpeg_url", { udid })
      .then(setStreamUrl)
      .catch(() => setStreamUrl(null));
  }, [udid]);

  const onPointer = React.useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!udid) return;
      const rect = event.currentTarget.getBoundingClientRect();
      const x = ((event.clientX - rect.left) / rect.width) * 390;
      const y = ((event.clientY - rect.top) / rect.height) * 844;
      void invokeGovernor("sim_tap", { udid, point: { x, y } }).catch(
        () => undefined,
      );
    },
    [udid],
  );

  return (
    <div
      className="flex flex-col items-center gap-3"
      data-testid="sim-face-mirroring"
    >
      <div
        className="relative h-80 w-40 cursor-none overflow-hidden rounded-[2rem] border-4 border-foreground/80 bg-zinc-900"
        data-testid="sim-bezel"
        onPointerDown={onPointer}
        onWheel={(event) => {
          if (!udid) return;
          void invokeGovernor("sim_scroll", {
            udid,
            deltaY: event.deltaY,
          }).catch(() => undefined);
        }}
        style={{
          transitionTimingFunction: `cubic-bezier(${ENTER_EASE.join(",")})`,
        }}
      >
        {streamUrl ? (
          <img
            alt="Simulator mirror"
            className="h-full w-full object-cover"
            data-testid="sim-mjpeg"
            src={streamUrl}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-2xs text-zinc-400">
            Stream paused
          </div>
        )}
      </div>
      <div className="flex items-center gap-1" data-testid="sim-control-bar">
        <IconBtn
          label="Home"
          onClick={() => udid && void invokeGovernor("sim_home", { udid })}
          testId="sim-home"
        >
          <Home className="h-3.5 w-3.5" />
        </IconBtn>
        <IconBtn
          label="Rotate"
          onClick={() => udid && void invokeGovernor("sim_rotate", { udid })}
          testId="sim-rotate"
        >
          <RotateCw className="h-3.5 w-3.5" />
        </IconBtn>
        <IconBtn
          label="Shot"
          onClick={() => {
            if (!udid) return;
            void (async () => {
              const png = await captureSimPng(udid);
              await postCaptureEvidence({
                channelId,
                threadRootId,
                kind: "shot",
                png,
                filename: "sim-shot.png",
              });
            })();
          }}
          testId="sim-shot"
        >
          <Camera className="h-3.5 w-3.5" />
        </IconBtn>
        <IconBtn
          label="Clip"
          onClick={() => {
            if (!udid) return;
            void (async () => {
              const png = await captureSimPng(udid);
              await postCaptureEvidence({
                channelId,
                threadRootId,
                kind: "clip",
                png,
                filename: "sim-clip.png",
              });
            })();
          }}
          testId="sim-clip"
        >
          <MoreHorizontal className="h-3.5 w-3.5" />
        </IconBtn>
      </div>
      {holding ? (
        <p className="text-2xs text-muted-foreground">
          Data: {formatBytes(holding.diskBytes)}
        </p>
      ) : null}
      <div className="flex gap-2">
        <Button
          data-testid="sim-erase"
          onClick={() => void simErase(channelId)}
          size="sm"
          variant="outline"
        >
          Erase
        </Button>
        <Button
          data-testid="sim-delete"
          onClick={() => void simDelete(channelId)}
          size="sm"
          variant="outline"
        >
          Delete
        </Button>
      </div>
    </div>
  );
}

function BridgeMissingCard({
  hint,
  message,
}: {
  hint: string | null;
  message: string | null;
}) {
  const command = hint ?? "brew install baguette";
  return (
    <div
      className="max-w-sm rounded-xl border border-border/60 bg-muted/30 p-4"
      data-testid="sim-face-bridge-missing"
    >
      <p className="text-sm font-medium text-foreground">
        Simulator bridge missing
      </p>
      <p className="mt-1 text-2xs text-muted-foreground">
        {message ??
          "Install a headless bridge to stream frames into this pane. Simulator.app is not the mirror."}
      </p>
      <pre className="mt-3 overflow-x-auto rounded-md bg-background p-2 text-2xs">
        {command}
      </pre>
      <Button
        className="mt-3"
        data-testid="sim-bridge-recheck"
        onClick={() => {
          void invokeGovernor("sim_bridge_status").catch(() => undefined);
        }}
        size="sm"
        type="button"
        variant="outline"
      >
        Recheck
      </Button>
    </div>
  );
}

function IdleStrip({
  channelId,
  deadlineMs,
  nowMs,
}: {
  channelId: string;
  deadlineMs: number;
  nowMs: number;
}) {
  const label = formatCountdown(deadlineMs, nowMs);
  return (
    <div
      className="flex shrink-0 items-center gap-2 border-t border-amber-500/40 bg-amber-500/10 px-3 py-1.5 text-2xs text-amber-800 dark:text-amber-200"
      data-testid="sim-idle-strip"
    >
      <span>Shuts down in {label} unless used</span>
      <Button
        className="ml-auto h-6"
        data-testid="sim-keep"
        onClick={() => void simKeep(channelId)}
        size="sm"
        type="button"
        variant="outline"
      >
        Keep
      </Button>
    </div>
  );
}

function IconBtn({
  children,
  label,
  onClick,
  testId,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  testId: string;
}) {
  return (
    <button
      aria-label={label}
      className="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
      data-testid={testId}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}
