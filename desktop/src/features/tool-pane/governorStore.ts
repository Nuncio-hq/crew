import * as React from "react";
import { listen } from "@tauri-apps/api/event";

import { invokeTauri } from "@/shared/api/tauri";
import { isTauri } from "@tauri-apps/api/core";

import {
  EMPTY_GOVERNOR_STATUS,
  type GovernorPolicy,
  type GovernorStatus,
} from "./types";

let snapshot: GovernorStatus = EMPTY_GOVERNOR_STATUS;
const listeners = new Set<() => void>();
let started = false;

function publish(next: GovernorStatus) {
  snapshot = next;
  for (const listener of listeners) listener();
}

export function applyGovernorStatus(next: GovernorStatus) {
  publish(next);
}

export function getGovernorStatus(): GovernorStatus {
  return snapshot;
}

export async function refreshGovernorStatus(): Promise<GovernorStatus> {
  if (!isTauri() && typeof window !== "undefined") {
    const invoke = window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
    if (invoke) {
      const next = (await invoke("governor_status", {})) as GovernorStatus;
      publish(next);
      return next;
    }
  }
  const next = await invokeTauri<GovernorStatus>("governor_status");
  publish(next);
  return next;
}

export async function startGovernorListener(): Promise<void> {
  if (started) return;
  started = true;
  await refreshGovernorStatus().catch(() => undefined);
  if (!isTauri()) return;
  await listen<GovernorStatus>("resource-governor", (event) => {
    publish(event.payload);
  });
}

export function useGovernorStatus(): GovernorStatus {
  React.useEffect(() => {
    void startGovernorListener();
  }, []);
  return React.useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => snapshot,
  );
}

export function resetGovernorStatusForTests(
  next: GovernorStatus = EMPTY_GOVERNOR_STATUS,
) {
  started = false;
  publish(next);
}

export async function invokeGovernor<T>(
  command: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!isTauri() && typeof window !== "undefined") {
    const invoke = window.__BUZZ_E2E_INVOKE_MOCK_COMMAND__;
    if (invoke) {
      return (await invoke(command, args)) as T;
    }
  }
  return invokeTauri<T>(command, args);
}

export async function setGovernorPolicy(
  policy: GovernorPolicy,
): Promise<GovernorStatus> {
  const next = await invokeGovernor<GovernorStatus>("governor_set_policy", {
    policy,
  });
  publish(next);
  return next;
}
