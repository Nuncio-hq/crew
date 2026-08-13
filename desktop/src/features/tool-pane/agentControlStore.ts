import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";

import { invokeTauri } from "@/shared/api/tauri";
import { postCaptureEvidence } from "./postEvidenceCapture";

export type LeaseView = {
  channelId: string;
  instrument: string;
  state: string;
  agentName: string | null;
  humanHeldUntilMs: number | null;
};

export type OverlayFrame = {
  instrument: string;
  tool: string;
  channelId: string;
  target?: {
    ref: string;
    bounds?: { x: number; y: number; w: number; h: number };
  };
  point?: { x: number; y: number };
  atMs: number;
};

export type PendingOrigin = {
  channelId: string;
  origin: string;
  agentName?: string | null;
};

export type AgentControlUi = {
  leases: LeaseView[];
  overlay: OverlayFrame | null;
  pendingOrigin: PendingOrigin | null;
};

const EMPTY: AgentControlUi = {
  leases: [],
  overlay: null,
  pendingOrigin: null,
};

let snapshot: AgentControlUi = EMPTY;
const listeners = new Set<() => void>();
let started = false;

function publish(next: AgentControlUi) {
  snapshot = next;
  for (const listener of listeners) listener();
}

export function applyAgentControlUi(next: AgentControlUi) {
  publish(next);
}

export function getAgentControlUi(): AgentControlUi {
  return snapshot;
}

export async function refreshAgentControlUi(): Promise<AgentControlUi> {
  if (!isTauri() && typeof window !== "undefined") {
    const invoke = window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
    if (invoke) {
      const next = (await invoke("agent_control_status", {})) as AgentControlUi;
      publish(next);
      return next;
    }
  }
  const next = await invokeTauri<AgentControlUi>("agent_control_status");
  publish(next);
  return next;
}

export async function startAgentControlListener(): Promise<void> {
  if (started) return;
  started = true;
  await refreshAgentControlUi().catch(() => undefined);
  if (!isTauri()) return;
  await listen<AgentControlUi>("agent-control", (event) => {
    publish({
      leases: event.payload.leases ?? [],
      overlay: event.payload.overlay ?? null,
      pendingOrigin: event.payload.pendingOrigin ?? null,
    });
  });
  await listen<{
    channelId: string;
    threadRootId?: string | null;
    kind: "shot" | "clip";
    pngBase64: string;
  }>("agent-control-evidence", (event) => {
    const raw = event.payload.pngBase64;
    const binary = atob(raw);
    const png: number[] = [];
    for (let i = 0; i < binary.length; i += 1) {
      png.push(binary.charCodeAt(i));
    }
    void postCaptureEvidence({
      channelId: event.payload.channelId,
      threadRootId: event.payload.threadRootId,
      kind: event.payload.kind,
      png,
      filename:
        event.payload.kind === "clip"
          ? "agent-record.png"
          : "agent-screenshot.png",
    }).catch(() => undefined);
  });
}

export function useAgentControlUi(): AgentControlUi {
  React.useEffect(() => {
    void startAgentControlListener();
  }, []);
  return React.useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => snapshot,
  );
}

export function resetAgentControlForTests(next: AgentControlUi = EMPTY) {
  started = false;
  publish(next);
}

export function leaseFor(
  ui: AgentControlUi,
  channelId: string,
  instrument: string,
): LeaseView | null {
  return (
    ui.leases.find(
      (lease) =>
        lease.channelId === channelId && lease.instrument === instrument,
    ) ?? null
  );
}
