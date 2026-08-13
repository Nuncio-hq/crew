/**
 * In-memory Resource Governor + Tool Pane Tauri commands for the E2E mock bridge.
 *
 * Seed via `window.__BUZZ_E2E_SET_GOVERNOR__` after boot, or
 * `__BUZZ_E2E_GOVERNOR__` in addInitScript.
 */

import { parse as parseYaml, stringify as stringifyYaml } from "yaml";

import {
  DEFAULT_GOVERNOR_POLICY,
  EMPTY_GOVERNOR_STATUS,
  type CanvasTooling,
  type GovernorStatus,
  type SimHolding,
} from "@/features/tool-pane/types";
import type { AgentControlUi } from "@/features/tool-pane/agentControlStore";
import { applyAgentControlUi } from "@/features/tool-pane/agentControlStore";

const TOOL_PANE_COMMANDS = new Set([
  "governor_status",
  "governor_set_policy",
  "governor_stop",
  "governor_start_dev_server",
  "governor_note_server_output",
  "sim_ensure_device",
  "sim_boot",
  "sim_shutdown",
  "sim_erase",
  "sim_delete",
  "sim_keep",
  "sim_set_pane_visible",
  "sim_bridge_status",
  "sim_mjpeg_url",
  "sim_tap",
  "sim_swipe",
  "sim_scroll",
  "sim_key",
  "sim_text",
  "sim_home",
  "sim_rotate",
  "sim_screenshot_png",
  "browser_open",
  "set_browser_bounds",
  "browser_close",
  "browser_devtools",
  "open_tool_pane_window",
  "crew_device_name_for",
  "probe_browser_backend",
  "get_canvas_tooling",
  "set_canvas_tooling",
  "agent_control_status",
  "agent_control_take_over",
  "agent_control_release",
  "agent_control_note_human",
  "agent_control_origin_decision",
  "terminal_attach",
  "terminal_input",
]);

export function isToolPaneCommand(command: string): boolean {
  return TOOL_PANE_COMMANDS.has(command);
}

let status: GovernorStatus = structuredClone(EMPTY_GOVERNOR_STATUS);
let canvasByChannel = new Map<string, string>();
const hiddenAt = new Map<string, number>();
let lastBrowserUrl: string | null = null;
let agentControl: AgentControlUi = {
  leases: [],
  overlay: null,
  pendingOrigin: null,
};

function publishAgentControl(): AgentControlUi {
  const next = structuredClone(agentControl);
  applyAgentControlUi(next);
  return next;
}

status.bridge = {
  availability: "available",
  binary: "baguette",
  path: "/usr/local/bin/baguette",
  installHint: "brew install baguette",
  message: null,
};
status.childWebviewAvailable = false;
status.nowMs = Date.now();

function publish(): GovernorStatus {
  status = {
    ...status,
    nowMs: Date.now(),
    bootedCount: status.sims.filter(
      (sim) => sim.lifecycle === "booted" || sim.lifecycle === "mirroring",
    ).length,
    streamCount: status.sims.filter((sim) => sim.mirroring).length,
    serverCount: status.servers.filter((server) => server.face === "running")
      .length,
    diskBytes: status.sims.reduce((sum, sim) => sum + sim.diskBytes, 0),
  };
  return structuredClone(status);
}

function deviceName(channelId: string): string {
  const prefix = channelId.replace(/-/g, "").slice(0, 8);
  return `crew-${prefix}`;
}

function holding(channelId: string): SimHolding | undefined {
  return status.sims.find((sim) => sim.channelId === channelId);
}

function ensureHolding(
  channelId: string,
  channelName?: string,
  deviceType?: string,
  runtime?: string,
): SimHolding {
  const existing = holding(channelId);
  if (existing) return existing;
  const next: SimHolding = {
    channelId,
    channelName: channelName ?? null,
    deviceName: deviceName(channelId),
    udid: null,
    lifecycle: "absent",
    deviceType: deviceType ?? "iPhone 16 Pro",
    runtime: runtime ?? "iOS 18",
    foreign: false,
    diskBytes: 0,
    lastUsedMs: Date.now(),
    idleDeadlineMs: null,
    paneVisible: false,
    mirroring: false,
    lastScreenshotDataUrl: null,
    bootElapsedMs: null,
  };
  status.sims = [...status.sims, next];
  return next;
}

function replaceSim(next: SimHolding) {
  status.sims = status.sims.map((sim) =>
    sim.channelId === next.channelId ? next : sim,
  );
}

export function handleToolPaneCommand(
  command: string,
  payload: unknown,
): unknown {
  const args = (payload ?? {}) as Record<string, unknown>;
  const input = (args.input as Record<string, unknown> | undefined) ?? args;

  switch (command) {
    case "governor_status":
      return publish();
    case "governor_set_policy": {
      status.policy = {
        ...status.policy,
        ...(args.policy as GovernorStatus["policy"]),
      };
      return publish();
    }
    case "governor_stop": {
      const kind = String(args.kind ?? "");
      const id = String(args.id ?? "");
      if (kind === "everything") {
        status.sims = status.sims.map((sim) => ({
          ...sim,
          lifecycle: sim.lifecycle === "absent" ? sim.lifecycle : "shutdown",
          mirroring: false,
          paneVisible: false,
          idleDeadlineMs: null,
        }));
        status.servers = [];
        status.webviews = [];
        status.capConflict = null;
      } else if (kind === "sim") {
        const sim = holding(id);
        if (sim) {
          replaceSim({
            ...sim,
            lifecycle: "shutdown",
            mirroring: false,
            paneVisible: false,
            idleDeadlineMs: null,
          });
        }
      } else if (kind === "server") {
        status.servers = status.servers.filter((server) => server.id !== id);
      }
      return publish();
    }
    case "sim_ensure_device": {
      ensureHolding(
        String(input.channelId),
        input.channelName as string | undefined,
        input.deviceType as string | undefined,
        input.runtime as string | undefined,
      );
      const sim = holding(String(input.channelId));
      if (sim && sim.lifecycle === "absent") {
        replaceSim({ ...sim, lifecycle: "shutdown" });
      }
      return publish();
    }
    case "sim_boot": {
      const channelId = String(input.channelId);
      const incomingName = String(input.channelName ?? channelId.slice(0, 8));
      const sim = ensureHolding(
        channelId,
        input.channelName as string | undefined,
        input.deviceType as string | undefined,
        input.runtime as string | undefined,
      );
      const booted = status.sims.filter(
        (entry) =>
          entry.lifecycle === "booted" || entry.lifecycle === "mirroring",
      );
      if (booted.length >= status.policy.maxBootedSims) {
        const victim = booted.find((entry) => !entry.mirroring) ?? null;
        if (!victim) {
          throw new Error("sim boot cap reached; visible mirror is protected");
        }
        status.capConflict = {
          kind: "sim",
          victimChannelId: victim.channelId,
          victimName: victim.channelName ?? victim.deviceName,
          incomingChannelId: channelId,
          incomingName,
          idleMs: Math.max(0, Date.now() - victim.lastUsedMs),
          keepToken: `keep-${victim.channelId}`,
        };
        throw new Error(
          `cap: Sim of ${status.capConflict.victimName} shuts down to make room for ${incomingName}`,
        );
      }
      replaceSim({
        ...sim,
        lifecycle: "booted",
        udid: sim.udid ?? `UDID-${deviceName(channelId)}`,
        lastUsedMs: Date.now(),
        idleDeadlineMs: Date.now() + status.policy.simIdleShutdownMs,
        diskBytes: sim.diskBytes || 450 * 1024 * 1024,
        bootElapsedMs: 0,
        mirroring: false,
      });
      return publish();
    }
    case "sim_shutdown": {
      const sim = holding(String(args.channelId));
      if (sim) {
        replaceSim({
          ...sim,
          lifecycle: "shutdown",
          mirroring: false,
          paneVisible: false,
          idleDeadlineMs: null,
        });
      }
      return publish();
    }
    case "sim_erase": {
      const sim = holding(String(args.channelId));
      if (sim) {
        replaceSim({
          ...sim,
          lifecycle: "shutdown",
          mirroring: false,
          diskBytes: 0,
        });
      }
      return publish();
    }
    case "sim_delete": {
      status.sims = status.sims.filter(
        (sim) => sim.channelId !== String(args.channelId),
      );
      return publish();
    }
    case "sim_keep": {
      const sim = holding(String(args.channelId));
      if (sim) {
        replaceSim({
          ...sim,
          lastUsedMs: Date.now(),
          idleDeadlineMs: Date.now() + status.policy.simIdleShutdownMs,
        });
      }
      if (status.capConflict?.victimChannelId === String(args.channelId)) {
        status.capConflict = null;
      }
      return publish();
    }
    case "sim_set_pane_visible": {
      const channelId = String(args.channelId);
      const visible = Boolean(args.visible);
      const sim = holding(channelId);
      if (!sim) return publish();
      if (visible) {
        hiddenAt.delete(channelId);
        const streams = status.sims.filter(
          (entry) => entry.mirroring && entry.channelId !== channelId,
        ).length;
        const canMirror = streams < status.policy.maxMirrorStreams;
        replaceSim({
          ...sim,
          paneVisible: true,
          lastUsedMs: Date.now(),
          idleDeadlineMs: Date.now() + status.policy.simIdleShutdownMs,
          lifecycle:
            sim.lifecycle === "booted" && canMirror
              ? "mirroring"
              : sim.lifecycle,
          mirroring: (sim.lifecycle === "booted" || sim.mirroring) && canMirror,
        });
      } else {
        hiddenAt.set(channelId, Date.now());
        replaceSim({ ...sim, paneVisible: false });
        window.setTimeout(() => {
          const current = holding(channelId);
          if (!current || current.paneVisible) return;
          replaceSim({
            ...current,
            mirroring: false,
            lifecycle:
              current.lifecycle === "mirroring" ? "booted" : current.lifecycle,
          });
          publish();
        }, status.policy.streamPauseHiddenMs);
      }
      return publish();
    }
    case "sim_bridge_status":
      return status.bridge;
    case "sim_mjpeg_url":
      return `http://127.0.0.1:9/sim/${String(args.udid)}/mjpeg`;
    case "sim_tap":
    case "sim_swipe":
    case "sim_scroll":
    case "sim_key":
    case "sim_text":
    case "sim_home":
    case "sim_rotate":
      return null;
    case "sim_screenshot_png":
      return [137, 80, 78, 71, 13, 10, 26, 10];
    case "browser_open": {
      lastBrowserUrl = String(args.url ?? "");
      const channelId = String(args.channelId);
      const existing = status.webviews.find(
        (view) => view.channelId === channelId,
      );
      if (existing) {
        status.webviews = status.webviews.map((view) =>
          view.channelId === channelId
            ? { ...view, url: lastBrowserUrl ?? view.url, hidden: false }
            : view,
        );
      } else {
        status.webviews = [
          ...status.webviews,
          {
            id: `wv-${channelId}`,
            channelId,
            url: lastBrowserUrl ?? "about:blank",
            hidden: false,
            hiddenSinceMs: null,
            backend: "window",
          },
        ];
      }
      publish();
      return `crew-browser-${channelId.replace(/-/g, "").slice(0, 12)}`;
    }
    case "set_browser_bounds":
      return null;
    case "browser_close": {
      const channelId = String(args.channelId);
      status.webviews = status.webviews.map((view) =>
        view.channelId === channelId
          ? { ...view, hidden: true, hiddenSinceMs: Date.now() }
          : view,
      );
      return publish();
    }
    case "browser_devtools":
    case "open_tool_pane_window":
      return null;
    case "crew_device_name_for":
      return deviceName(String(args.channelId ?? args.channel_id ?? ""));
    case "probe_browser_backend":
      return "window";
    case "governor_start_dev_server": {
      const channelId = String(input.channelId);
      const subject = String(input.subject ?? "checkout");
      const id = `${channelId}:${subject}`;
      const port = 4173;
      const command = String(input.command ?? "pnpm dev --port $PORT").replace(
        "$PORT",
        String(port),
      );
      status.servers = [
        ...status.servers.filter((server) => server.id !== id),
        {
          id,
          channelId,
          subject,
          command,
          port,
          url: `http://127.0.0.1:${port}`,
          face: "running",
          uptimeMs: 0,
          idleDeadlineMs: Date.now() + status.policy.devServerIdleMs,
          lastLog: ["Local: http://127.0.0.1:4173"],
          portNote: null,
          crashCount: 0,
          cwd: String(input.cwd ?? "."),
        },
      ];
      return publish();
    }
    case "governor_note_server_output": {
      const serverId = String(args.serverId ?? args.server_id ?? "");
      const line = String(args.line ?? "");
      status.servers = status.servers.map((server) =>
        server.id === serverId
          ? { ...server, lastLog: [...server.lastLog, line].slice(-30) }
          : server,
      );
      return publish();
    }
    case "get_canvas_tooling": {
      const content = canvasByChannel.get(String(args.channelId)) ?? "";
      return parseTooling(content);
    }
    case "set_canvas_tooling": {
      const channelId = String(args.channelId);
      const tooling = args.tooling as CanvasTooling;
      const blob = JSON.stringify(tooling).toLowerCase();
      if (blob.includes("udid")) {
        throw new Error("tooling must store intent, not a device UDID");
      }
      const current = canvasByChannel.get(channelId) ?? "";
      canvasByChannel.set(channelId, writeTooling(current, tooling));
      return { ok: true, event_id: "a".repeat(64) };
    }
    case "terminal_attach":
      return {
        sessionId: "e2e-term-session",
        subscriptionId: "e2e-term-sub",
        viewport: { generation: 1, columns: 80, screenLines: 24 },
      };
    case "terminal_input":
      return null;
    case "agent_control_status":
      return publishAgentControl();
    case "agent_control_take_over": {
      const channelId = String(input.channelId ?? "");
      const instrument = String(input.instrument ?? "browser");
      agentControl.leases = agentControl.leases.map((lease) =>
        lease.channelId === channelId && lease.instrument === instrument
          ? {
              ...lease,
              state: "humanHeld",
              humanHeldUntilMs: Date.now() + 10_000,
            }
          : lease,
      );
      return publishAgentControl();
    }
    case "agent_control_release": {
      const channelId = String(input.channelId ?? "");
      const instrument = String(input.instrument ?? "browser");
      agentControl.leases = agentControl.leases.filter(
        (lease) =>
          !(lease.channelId === channelId && lease.instrument === instrument),
      );
      return publishAgentControl();
    }
    case "agent_control_note_human": {
      const channelId = String(input.channelId ?? "");
      const instrument = String(input.instrument ?? "browser");
      const existing = agentControl.leases.find(
        (lease) =>
          lease.channelId === channelId && lease.instrument === instrument,
      );
      if (existing) {
        existing.state = "humanHeld";
        existing.humanHeldUntilMs = Date.now() + 10_000;
      } else {
        agentControl.leases.push({
          channelId,
          instrument,
          state: "humanHeld",
          agentName: "Hermes",
          humanHeldUntilMs: Date.now() + 10_000,
        });
      }
      return publishAgentControl();
    }
    case "agent_control_origin_decision": {
      const decision = String(input.decision ?? args.decision ?? "");
      const origin = String(input.origin ?? args.origin ?? "");
      const channelId = String(input.channelId ?? args.channelId ?? "");
      if (decision === "allow_domain" && origin) {
        const current = parseTooling(canvasByChannel.get(channelId) ?? "");
        const allowlist = [...(current?.browserAllowlist ?? []), origin];
        canvasByChannel.set(
          channelId,
          writeTooling(canvasByChannel.get(channelId) ?? "", {
            ...(current ?? {}),
            browserAllowlist: allowlist,
          }),
        );
      }
      agentControl.pendingOrigin = null;
      return { ok: true };
    }
    default:
      throw new Error(`Unhandled tool-pane command: ${command}`);
  }
}

export function setE2eGovernorStatus(
  patch: Partial<GovernorStatus>,
): GovernorStatus {
  status = {
    ...status,
    ...patch,
    policy: { ...status.policy, ...(patch.policy ?? {}) },
    bridge: { ...status.bridge, ...(patch.bridge ?? {}) },
  };
  return publish();
}

export function resetE2eGovernor(): void {
  status = structuredClone(EMPTY_GOVERNOR_STATUS);
  status.bridge = {
    availability: "available",
    binary: "baguette",
    path: "/usr/local/bin/baguette",
    installHint: "brew install baguette",
    message: null,
  };
  status.childWebviewAvailable = false;
  status.policy = { ...DEFAULT_GOVERNOR_POLICY };
  canvasByChannel = new Map();
  hiddenAt.clear();
  lastBrowserUrl = null;
  agentControl = { leases: [], overlay: null, pendingOrigin: null };
  publishAgentControl();
}

export function setE2eAgentControl(next: AgentControlUi): AgentControlUi {
  agentControl = structuredClone(next);
  return publishAgentControl();
}

export function seedE2eCanvas(channelId: string, content: string): void {
  canvasByChannel.set(channelId, content);
}

export function getE2eLastBrowserUrl(): string | null {
  return lastBrowserUrl;
}

function parseTooling(content: string): CanvasTooling | null {
  const start = content.indexOf("```crew");
  if (start < 0) return null;
  const bodyStart = content.indexOf("\n", start);
  const end = content.indexOf("```", bodyStart + 1);
  if (bodyStart < 0 || end < 0) return null;
  try {
    const doc = parseYaml(content.slice(bodyStart + 1, end)) as {
      tooling?: CanvasTooling;
    } | null;
    return doc?.tooling ?? null;
  } catch {
    return null;
  }
}

function writeTooling(content: string, tooling: CanvasTooling): string {
  const start = content.indexOf("```crew");
  if (start < 0) {
    return `${content}\n\n\`\`\`crew\n${stringifyYaml({ tooling })}\`\`\`\n`;
  }
  const bodyStart = content.indexOf("\n", start);
  const end = content.indexOf("```", bodyStart + 1);
  if (bodyStart < 0 || end < 0) return content;
  const doc =
    (parseYaml(content.slice(bodyStart + 1, end)) as Record<string, unknown>) ??
    {};
  doc.tooling = tooling;
  return `${content.slice(0, start)}\`\`\`crew\n${stringifyYaml(doc)}\`\`\`${content.slice(end + 3)}`;
}
