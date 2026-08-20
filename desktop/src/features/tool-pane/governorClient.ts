import type { CanvasTooling, GovernorStatus, SimHolding } from "./types";
import { invokeGovernor, applyGovernorStatus } from "./governorStore";
import { toolingHasUdid } from "./canvasTooling";

export async function simEnsureDevice(input: {
  channelId: string;
  channelName?: string;
  deviceType?: string;
  runtime?: string;
}): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("sim_ensure_device", {
    input,
  });
  applyGovernorStatus(next);
  return next;
}

export async function simBoot(input: {
  channelId: string;
  channelName?: string;
  deviceType?: string;
  runtime?: string;
}): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("sim_boot", { input });
  applyGovernorStatus(next);
  return next;
}

export async function simShutdown(channelId: string): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("sim_shutdown", {
    channelId,
  });
  applyGovernorStatus(next);
  return next;
}

export async function simKeep(channelId: string): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("sim_keep", { channelId });
  applyGovernorStatus(next);
  return next;
}

export async function simSetPaneVisible(
  channelId: string,
  visible: boolean,
): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("sim_set_pane_visible", {
    channelId,
    visible,
  });
  applyGovernorStatus(next);
  return next;
}

export async function simErase(channelId: string): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("sim_erase", { channelId });
  applyGovernorStatus(next);
  return next;
}

export async function simDelete(channelId: string): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("sim_delete", {
    channelId,
  });
  applyGovernorStatus(next);
  return next;
}

export async function governorStop(
  kind: "sim" | "server" | "webview" | "everything",
  id: string,
): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("governor_stop", {
    kind,
    id,
  });
  applyGovernorStatus(next);
  return next;
}

export async function startDevServer(input: {
  channelId: string;
  subject: string;
  command: string;
  cwd: string;
  readyPattern?: string;
}): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>(
    "governor_start_dev_server",
    { input },
  );
  applyGovernorStatus(next);
  return next;
}

export async function browserOpen(
  channelId: string,
  url: string,
): Promise<string> {
  return invokeGovernor<string>("browser_open", { channelId, url });
}

export async function setBrowserBounds(
  channelId: string,
  x: number,
  y: number,
  width: number,
  height: number,
): Promise<void> {
  await invokeGovernor("set_browser_bounds", {
    channelId,
    x,
    y,
    width,
    height,
  });
}

export async function browserClose(channelId: string): Promise<void> {
  await invokeGovernor("browser_close", { channelId });
}

export async function browserDevtools(channelId: string): Promise<void> {
  await invokeGovernor("browser_devtools", { channelId });
}

export async function browserBack(channelId: string): Promise<void> {
  await invokeGovernor("browser_back", { channelId });
}

export async function browserForward(channelId: string): Promise<void> {
  await invokeGovernor("browser_forward", { channelId });
}

export async function browserReload(channelId: string): Promise<void> {
  await invokeGovernor("browser_reload", { channelId });
}

export async function openToolPaneWindow(channelId: string): Promise<void> {
  await invokeGovernor("open_tool_pane_window", { channelId });
}

export async function getCanvasTooling(
  channelId: string,
): Promise<CanvasTooling | null> {
  return invokeGovernor<CanvasTooling | null>("get_canvas_tooling", {
    channelId,
  });
}

export async function setCanvasTooling(
  channelId: string,
  tooling: CanvasTooling,
): Promise<{ ok: boolean; eventId?: string }> {
  if (toolingHasUdid(tooling)) {
    throw new Error("tooling must store intent, not a device UDID");
  }
  return invokeGovernor<{ ok: boolean; eventId?: string }>(
    "set_canvas_tooling",
    { channelId, tooling },
  );
}

export function holdingForChannel(
  status: GovernorStatus,
  channelId: string,
): SimHolding | undefined {
  return status.sims.find((sim) => sim.channelId === channelId);
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const mb = bytes / (1024 * 1024);
  if (mb < 1024) return `~${mb.toFixed(0)} MB`;
  return `~${(mb / 1024).toFixed(1)} GB`;
}

export function formatCountdown(
  deadlineMs: number | null,
  nowMs: number,
): string | null {
  if (deadlineMs == null) return null;
  const remain = Math.max(0, deadlineMs - nowMs);
  const totalSeconds = Math.floor(remain / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}
