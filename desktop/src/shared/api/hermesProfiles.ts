/**
 * Hermes profile lifecycle IPC (Phase 03 / feature 0001).
 *
 * Mirrors `hermes_profile_lifecycle` Tauri commands. Create/delete are
 * explicit manager actions only (D-019 item 6).
 */

import { invokeTauri } from "./tauri";

/** Tagged result from create/delete — status drives UI copy. */
export type HermesProfileLifecycleResult =
  | { status: "ok"; name: string }
  | { status: "already_gone"; name: string }
  | { status: "invalid_name"; name: string; message: string }
  | { status: "already_exists"; name: string; message: string }
  | { status: "does_not_exist"; name: string; message: string }
  | { status: "binary_missing"; message: string }
  | { status: "failed"; name: string; message: string };

export function hermesProfileLifecycleSuccess(
  result: HermesProfileLifecycleResult,
): boolean {
  return result.status === "ok" || result.status === "already_gone";
}

export function hermesProfileLifecycleMessage(
  result: HermesProfileLifecycleResult,
): string {
  switch (result.status) {
    case "ok":
      return `Hermes profile '${result.name}' ready.`;
    case "already_gone":
      return `Hermes profile '${result.name}' was already gone.`;
    case "binary_missing":
      return result.message;
    default:
      return result.message;
  }
}

export async function listHermesProfiles(): Promise<string[]> {
  return invokeTauri<string[]>("list_hermes_profiles");
}

export async function createHermesProfile(
  name: string,
): Promise<HermesProfileLifecycleResult> {
  return invokeTauri<HermesProfileLifecycleResult>("create_hermes_profile", {
    name,
  });
}

export async function deleteHermesProfile(
  name: string,
): Promise<HermesProfileLifecycleResult> {
  return invokeTauri<HermesProfileLifecycleResult>("delete_hermes_profile", {
    name,
  });
}

/** Auditable command line shown next to create-in-place (D-019 / P-6). */
export function hermesProfileCreateCommandLine(name: string): string {
  const trimmed = name.trim() || "<name>";
  return `hermes profile create ${trimmed} --no-alias`;
}

/** Auditable command line for offboarding delete (always -y). */
export function hermesProfileDeleteCommandLine(name: string): string {
  return `hermes profile delete ${name.trim()} -y`;
}
