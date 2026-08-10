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

export type HermesProfileArchiveManifest = {
  schema_version: number;
  profile: string;
  archived_at: string;
  bound_agent_name: string | null;
  bound_agent_pubkey: string | null;
  offboard_reason: string | null;
  exclusions: string[];
  skipped_links: string[];
  entry_count: number;
  included_bytes: number;
};

export type HermesProfileArchiveListing = {
  id: string;
  archive_bytes: number;
  manifest: HermesProfileArchiveManifest;
};

export type HermesProfileArchiveEstimate = {
  included_bytes: number;
  excluded_bytes: number;
  entry_count: number;
};

export type HermesProfileArchiveResult =
  | {
      status: "archived";
      id: string;
      profile: string;
      included_bytes: number;
      archive_bytes: number;
      skipped_link_count: number;
    }
  | { status: "restored"; id: string; profile: string }
  | { status: "permanently_deleted"; id: string; profile: string }
  | { status: "invalid_name"; profile: string; message: string }
  | { status: "does_not_exist"; id: string; message: string }
  | { status: "collision"; profile: string; message: string }
  | {
      status: "agent_running";
      profile: string;
      agent_name: string;
      agent_pubkey: string;
      message: string;
    }
  | { status: "confirmation_mismatch"; profile: string; message: string }
  | {
      status: "failed";
      profile: string | null;
      id: string | null;
      message: string;
    };

export async function estimateHermesProfileArchive(
  profile: string,
): Promise<HermesProfileArchiveEstimate> {
  return invokeTauri("estimate_hermes_profile_archive", { profile });
}

export async function archiveHermesProfile(
  profile: string,
  reason?: string,
): Promise<HermesProfileArchiveResult> {
  return invokeTauri("archive_hermes_profile", { profile, reason });
}

export async function listHermesProfileArchives(): Promise<
  HermesProfileArchiveListing[]
> {
  return invokeTauri("list_hermes_profile_archives");
}

export async function restoreHermesProfileArchive(
  id: string,
): Promise<HermesProfileArchiveResult> {
  return invokeTauri("restore_hermes_profile_archive", { id });
}

export async function permanentlyDeleteHermesProfileArchive(
  id: string,
  confirmationToken: string,
): Promise<HermesProfileArchiveResult> {
  return invokeTauri("permanently_delete_hermes_profile_archive", {
    id,
    confirmationToken,
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
