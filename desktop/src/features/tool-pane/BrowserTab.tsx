import {
  Bug,
  Camera,
  ChevronLeft,
  ChevronRight,
  Monitor,
  RotateCw,
} from "lucide-react";
import * as React from "react";

import { useMyRelayMembershipQuery } from "@/features/community-members/hooks";
import { TerminalConnection } from "@/features/terminal/terminalClient";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { cn } from "@/shared/lib/cn";

import {
  browserClose,
  browserDevtools,
  browserOpen,
  formatCountdown,
  getCanvasTooling,
  setBrowserBounds,
  setCanvasTooling,
  startDevServer,
} from "./governorClient";
import { invokeGovernor, useGovernorStatus } from "./governorStore";
import { captureSimPng, postCaptureEvidence } from "./postEvidenceCapture";
import {
  isAgentControlChromeTarget,
  leaseFor,
  useAgentControlUi,
} from "./agentControlStore";
import { DrivingBanner } from "./DrivingBanner";
import { GhostCursorOverlay } from "./GhostCursorOverlay";
import { PendingOriginPrompt } from "./OriginApprovalCard";
import {
  VIEWPORT_PRESETS,
  type CanvasTooling,
  type DevServerFace,
  type DevServerHolding,
} from "./types";

type SubjectKind = "worktree" | "checkout" | "custom";

export function BrowserTab({
  channelId,
  channelName,
  threadRootId,
  worktreePath,
  checkoutPath,
}: {
  channelId: string;
  channelName: string;
  threadRootId?: string | null;
  worktreePath?: string | null;
  checkoutPath?: string | null;
}) {
  const status = useGovernorStatus();
  const control = useAgentControlUi();
  const lease = leaseFor(control, channelId, "browser");
  const membership = useMyRelayMembershipQuery();
  const role = membership.data?.role;
  const isOwner = role === "owner" || role == null;
  const [tooling, setTooling] = React.useState<CanvasTooling | null>(null);
  const [subject, setSubject] = React.useState<SubjectKind>(() =>
    worktreePath ? "worktree" : "checkout",
  );
  const [customUrl, setCustomUrl] = React.useState("http://127.0.0.1:5173");
  const [viewport, setViewport] =
    React.useState<(typeof VIEWPORT_PRESETS)[number]["id"]>("desktop");
  const frameRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    void getCanvasTooling(channelId)
      .then(setTooling)
      .catch(() => setTooling(null));
  }, [channelId]);

  const server = status.servers.find((entry) => entry.channelId === channelId);
  const url = resolveBrowserUrl({
    subject,
    customUrl,
    server,
    worktreePath,
    checkoutPath,
  });

  React.useEffect(() => {
    if (!url || !tooling?.devServer) return;
    void browserOpen(channelId, url).catch(() => undefined);
    return () => {
      void browserClose(channelId).catch(() => undefined);
    };
  }, [channelId, url, tooling?.devServer]);

  React.useEffect(() => {
    const node = frameRef.current;
    if (!node) return;
    const sync = () => {
      const rect = node.getBoundingClientRect();
      void setBrowserBounds(channelId, rect.x, rect.y, rect.width, rect.height);
    };
    const observer = new ResizeObserver(sync);
    observer.observe(node);
    sync();
    return () => observer.disconnect();
  }, [channelId]);

  const showSetup = !tooling?.devServer && subject !== "custom";

  return (
    <div
      className="relative flex min-h-0 flex-1 flex-col"
      data-testid="tool-pane-browser"
      onPointerDown={(event) => {
        if (isAgentControlChromeTarget(event.target)) return;
        void invokeGovernor("agent_control_note_human", {
          input: { channelId, instrument: "browser" },
        });
      }}
    >
      <DrivingBanner instrument="browser" lease={lease} />
      {control.pendingOrigin?.channelId === channelId ? (
        <PendingOriginPrompt
          agentName={control.pendingOrigin.agentName}
          channelId={channelId}
          origin={control.pendingOrigin.origin}
        />
      ) : null}
      <GhostCursorOverlay
        channelId={channelId}
        instrument="browser"
        overlay={control.overlay}
      />
      <BrowserToolbar
        customUrl={customUrl}
        onCustomUrl={setCustomUrl}
        onDevtools={() =>
          void browserDevtools(channelId).catch(() => undefined)
        }
        onShot={() => {
          void (async () => {
            const png = await captureSimPng("browser");
            await postCaptureEvidence({
              channelId,
              threadRootId,
              kind: "shot",
              png,
              filename: "browser-shot.png",
            });
          })();
        }}
        onSubject={setSubject}
        subject={subject}
        viewport={viewport}
        onViewport={setViewport}
        worktreePath={worktreePath}
        checkoutPath={checkoutPath}
      />
      <div
        className="flex min-h-0 flex-1 flex-col items-center justify-center p-3"
        ref={frameRef}
      >
        {showSetup ? (
          <DevServerSetupCard
            channelId={channelId}
            isOwner={isOwner}
            onSaved={setTooling}
          />
        ) : (
          <BrowserPreview
            backend={status.childWebviewAvailable ? "child" : "window"}
            url={url}
            width={
              VIEWPORT_PRESETS.find((preset) => preset.id === viewport)
                ?.width ?? 1280
            }
          />
        )}
      </div>
      {server ? (
        <ServerStrip
          channelName={channelName}
          nowMs={status.nowMs}
          server={server}
        />
      ) : tooling?.devServer && !showSetup ? (
        <div
          className="flex shrink-0 items-center gap-2 border-t border-border/60 px-3 py-1.5 text-2xs"
          data-testid="browser-server-strip"
          data-server-face="idle"
        >
          <span>Dev server stopped</span>
          <Button
            className="ml-auto h-6"
            data-testid="browser-start-server"
            onClick={() => {
              void launchDevServer({
                channelId,
                channelName,
                checkoutPath,
                command: tooling.devServer?.command ?? "pnpm dev --port $PORT",
                readyPattern: tooling.devServer?.readyPattern,
                subject,
                worktreePath,
              });
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            Start
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function BrowserToolbar({
  checkoutPath,
  customUrl,
  onCustomUrl,
  onDevtools,
  onShot,
  onSubject,
  onViewport,
  subject,
  viewport,
  worktreePath,
}: {
  checkoutPath?: string | null;
  customUrl: string;
  onCustomUrl: (value: string) => void;
  onDevtools: () => void;
  onShot: () => void;
  onSubject: (kind: SubjectKind) => void;
  onViewport: (id: (typeof VIEWPORT_PRESETS)[number]["id"]) => void;
  subject: SubjectKind;
  viewport: (typeof VIEWPORT_PRESETS)[number]["id"];
  worktreePath?: string | null;
}) {
  return (
    <div
      className="flex shrink-0 items-center gap-1 border-b border-border/60 px-2 py-1.5"
      data-testid="browser-toolbar"
    >
      <ToolbarIcon label="Back" testId="browser-back">
        <ChevronLeft className="h-3.5 w-3.5" />
      </ToolbarIcon>
      <ToolbarIcon label="Forward" testId="browser-forward">
        <ChevronRight className="h-3.5 w-3.5" />
      </ToolbarIcon>
      <ToolbarIcon label="Reload" testId="browser-reload">
        <RotateCw className="h-3.5 w-3.5" />
      </ToolbarIcon>
      <select
        className="h-7 max-w-[12rem] rounded-md border border-border bg-background px-1.5 text-2xs"
        data-testid="browser-subject"
        onChange={(event) => onSubject(event.target.value as SubjectKind)}
        value={subject}
      >
        <option disabled={!worktreePath} value="worktree">
          Dev server (worktree {shortPath(worktreePath)})
        </option>
        <option disabled={!checkoutPath} value="checkout">
          Dev server (channel checkout)
        </option>
        <option value="custom">Custom URL…</option>
      </select>
      <Input
        className="h-7 flex-1 text-2xs"
        data-testid="browser-url"
        onChange={(event) => onCustomUrl(event.target.value)}
        value={customUrl}
      />
      <select
        className="h-7 rounded-md border border-border bg-background px-1.5 text-2xs"
        data-testid="browser-viewport"
        onChange={(event) =>
          onViewport(
            event.target.value as (typeof VIEWPORT_PRESETS)[number]["id"],
          )
        }
        value={viewport}
      >
        {VIEWPORT_PRESETS.map((preset) => (
          <option key={preset.id} value={preset.id}>
            {preset.label}
          </option>
        ))}
      </select>
      <ToolbarIcon
        label="Devtools"
        onClick={onDevtools}
        testId="browser-devtools"
      >
        <Bug className="h-3.5 w-3.5" />
      </ToolbarIcon>
      <ToolbarIcon label="Shot" onClick={onShot} testId="browser-shot">
        <Camera className="h-3.5 w-3.5" />
      </ToolbarIcon>
    </div>
  );
}

function DevServerSetupCard({
  channelId,
  isOwner,
  onSaved,
}: {
  channelId: string;
  isOwner: boolean;
  onSaved: (tooling: CanvasTooling) => void;
}) {
  const [command, setCommand] = React.useState("pnpm dev --port $PORT");
  const [readyPattern, setReadyPattern] = React.useState("Local:");
  const [error, setError] = React.useState<string | null>(null);
  return (
    <div
      className="max-w-sm rounded-xl border border-border/60 bg-muted/30 p-4"
      data-testid="browser-setup-card"
    >
      <p className="text-sm font-medium text-foreground">
        Set up a Crew-owned dev server
      </p>
      <p className="mt-1 text-2xs text-muted-foreground">
        Command runs as a labeled Buzz Term session. `$PORT` is allocated by the
        Resource Governor. Intent is signed onto the channel canvas.
      </p>
      {isOwner ? (
        <>
          <Input
            className="mt-3 h-8 text-2xs"
            data-testid="browser-setup-command"
            onChange={(event) => setCommand(event.target.value)}
            value={command}
          />
          <Input
            className="mt-2 h-8 text-2xs"
            data-testid="browser-setup-ready"
            onChange={(event) => setReadyPattern(event.target.value)}
            value={readyPattern}
          />
          {error ? (
            <p className="mt-2 text-2xs text-destructive">{error}</p>
          ) : null}
          <Button
            className="mt-3"
            data-testid="browser-setup-save"
            onClick={() => {
              const tooling: CanvasTooling = {
                devServer: { command, readyPattern },
              };
              void setCanvasTooling(channelId, tooling)
                .then(() => onSaved(tooling))
                .catch((err: unknown) =>
                  setError(
                    err instanceof Error ? err.message : "Could not save",
                  ),
                );
            }}
            type="button"
          >
            Save to canvas
          </Button>
        </>
      ) : (
        <p
          className="mt-3 text-2xs text-muted-foreground"
          data-testid="browser-setup-owner-only"
        >
          Only the channel owner can write `tooling.devServer`.
        </p>
      )}
    </div>
  );
}

function BrowserPreview({
  backend,
  url,
  width,
}: {
  backend: string;
  url: string | null;
  width: number;
}) {
  return (
    <div
      className="flex h-full w-full flex-col items-center justify-center rounded-md border border-dashed border-border/80 bg-muted/20"
      data-browser-backend={backend}
      data-testid="browser-preview"
      style={{ maxWidth: width }}
    >
      <Monitor className="h-6 w-6 text-muted-foreground" />
      <p className="mt-2 text-2xs text-muted-foreground">
        {backend === "child"
          ? "Child webview"
          : "Companion window (huddle precedent)"}
      </p>
      <p
        className="mt-1 max-w-full truncate px-3 font-mono text-2xs"
        data-testid="browser-preview-url"
      >
        {url ?? "No URL"}
      </p>
    </div>
  );
}

function ServerStrip({
  channelName,
  nowMs,
  server,
}: {
  channelName: string;
  nowMs: number;
  server: DevServerHolding;
}) {
  const face: DevServerFace = server.face;
  const countdown = formatCountdown(server.idleDeadlineMs, nowMs);
  return (
    <div
      className={cn(
        "flex shrink-0 flex-col gap-1 border-t px-3 py-1.5 text-2xs",
        face === "crashed" || face === "portConflict"
          ? "border-destructive/40 bg-destructive/10"
          : face === "idleStop"
            ? "border-amber-500/40 bg-amber-500/10"
            : "border-border/60",
      )}
      data-server-face={face}
      data-testid="browser-server-strip"
    >
      <div className="flex items-center gap-2">
        <span>
          {face === "running"
            ? `Running on :${server.port}`
            : face === "idleStop"
              ? `Idle stop in ${countdown} · #${channelName}`
              : face === "crashed"
                ? `Crashed after ${server.crashCount} restart(s)`
                : face === "portConflict"
                  ? (server.portNote ?? "Port conflict")
                  : "Setup"}
        </span>
        {server.url ? (
          <span className="font-mono text-muted-foreground">{server.url}</span>
        ) : null}
      </div>
      {face === "crashed" ? (
        <>
          <pre className="max-h-24 overflow-auto rounded bg-background p-1 font-mono">
            {server.lastLog.slice(-30).join("\n")}
          </pre>
          <Button
            className="h-6 self-start"
            data-testid="browser-server-restart"
            onClick={() => {
              void invokeGovernor("governor_start_dev_server", {
                input: {
                  channelId: server.channelId,
                  subject: server.subject,
                  command: server.command,
                  cwd: server.cwd,
                },
              }).catch(() => undefined);
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            Restart
          </Button>
        </>
      ) : null}
    </div>
  );
}

function ToolbarIcon({
  children,
  label,
  onClick,
  testId,
}: {
  children: React.ReactNode;
  label: string;
  onClick?: () => void;
  testId: string;
}) {
  return (
    <button
      aria-label={label}
      className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
      data-testid={testId}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}

function shortPath(path: string | null | undefined): string {
  if (!path) return "…";
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? "…";
}

function resolveBrowserUrl(input: {
  subject: SubjectKind;
  customUrl: string;
  server: DevServerHolding | undefined;
  worktreePath?: string | null;
  checkoutPath?: string | null;
}): string | null {
  if (input.subject === "custom") return input.customUrl;
  return input.server?.url ?? null;
}

async function launchDevServer(input: {
  channelId: string;
  channelName: string;
  command: string;
  cwd?: string | null;
  readyPattern?: string;
  subject: SubjectKind;
  worktreePath?: string | null;
  checkoutPath?: string | null;
}) {
  const cwd =
    input.subject === "worktree"
      ? (input.worktreePath ?? input.checkoutPath ?? ".")
      : (input.checkoutPath ?? input.worktreePath ?? ".");
  const status = await startDevServer({
    channelId: input.channelId,
    subject: input.subject,
    command: input.command,
    cwd,
    readyPattern: input.readyPattern,
  });
  const holding = status.servers.find(
    (entry) => entry.channelId === input.channelId,
  );
  try {
    const connection = await TerminalConnection.attach(
      {
        channelId: input.channelId,
        channelName: `dev-server:${input.channelName}`,
        columns: 80,
        rows: 24,
        pixelWidth: 640,
        pixelHeight: 480,
        npub: "",
        relayUrl: "",
      },
      () => undefined,
      () => undefined,
    );
    await connection.input(`${holding?.command ?? input.command}\n`);
  } catch {
    // Term PTY is unavailable in some test hosts; governor still owns the holding.
  }
}
