import { invokeTauri } from "@/shared/api/tauri";

export type CoworkVersionKind = "baseline" | "external" | "turn" | "restore";

export type CoworkVersionEntry = {
  id: string;
  kind: CoworkVersionKind;
  summary: string;
  timestamp: number;
  agentName: string | null;
  threadTitle: string | null;
  threadId: string | null;
  filesChanged: string[];
};

export type CoworkExclusionNotice = {
  path: string;
  sizeBytes: number;
};

export type CoworkVersionsSnapshot = {
  versions: CoworkVersionEntry[];
  excluded: CoworkExclusionNotice[];
  notice: string | null;
  rebuilt: boolean;
  sizeThresholdBytes: number;
};

type RawSnapshot = {
  versions?: Array<{
    id?: string;
    kind?: CoworkVersionKind;
    summary?: string;
    timestamp?: number;
    agentName?: string | null;
    agent_name?: string | null;
    threadTitle?: string | null;
    thread_title?: string | null;
    threadId?: string | null;
    thread_id?: string | null;
    filesChanged?: string[];
    files_changed?: string[];
  }>;
  excluded?: Array<{
    path?: string;
    sizeBytes?: number;
    size_bytes?: number;
  }>;
  notice?: string | null;
  rebuilt?: boolean;
  sizeThresholdBytes?: number;
  size_threshold_bytes?: number;
};

function normalize(raw: RawSnapshot): CoworkVersionsSnapshot {
  return {
    versions: (raw.versions ?? []).map((entry) => ({
      id: entry.id ?? "",
      kind: entry.kind ?? "external",
      summary: entry.summary ?? "",
      timestamp: entry.timestamp ?? 0,
      agentName: entry.agentName ?? entry.agent_name ?? null,
      threadTitle: entry.threadTitle ?? entry.thread_title ?? null,
      threadId: entry.threadId ?? entry.thread_id ?? null,
      filesChanged: entry.filesChanged ?? entry.files_changed ?? [],
    })),
    excluded: (raw.excluded ?? []).map((item) => ({
      path: item.path ?? "",
      sizeBytes: item.sizeBytes ?? item.size_bytes ?? 0,
    })),
    notice: raw.notice ?? null,
    rebuilt: raw.rebuilt ?? false,
    sizeThresholdBytes:
      raw.sizeThresholdBytes ?? raw.size_threshold_bytes ?? 52_428_800,
  };
}

export async function initCoworkHistory(input: {
  repoAddress: string;
  folder: string;
}): Promise<CoworkVersionsSnapshot> {
  return normalize(
    await invokeTauri<RawSnapshot>("init_cowork_history", {
      repoAddress: input.repoAddress,
      folder: input.folder,
    }),
  );
}

export async function listCoworkVersions(input: {
  repoAddress: string;
  folder: string;
}): Promise<CoworkVersionsSnapshot> {
  return normalize(
    await invokeTauri<RawSnapshot>("list_cowork_versions", {
      repoAddress: input.repoAddress,
      folder: input.folder,
    }),
  );
}

export async function restoreCoworkFile(input: {
  repoAddress: string;
  folder: string;
  commit: string;
  relativePath: string;
}): Promise<CoworkVersionsSnapshot> {
  return normalize(
    await invokeTauri<RawSnapshot>("restore_cowork_file", {
      repoAddress: input.repoAddress,
      folder: input.folder,
      commit: input.commit,
      relativePath: input.relativePath,
    }),
  );
}

export async function restoreCoworkFolder(input: {
  repoAddress: string;
  folder: string;
  commit: string;
}): Promise<CoworkVersionsSnapshot> {
  return normalize(
    await invokeTauri<RawSnapshot>("restore_cowork_folder", {
      repoAddress: input.repoAddress,
      folder: input.folder,
      commit: input.commit,
    }),
  );
}

export async function compactCoworkHistory(input: {
  repoAddress: string;
  folder: string;
}): Promise<CoworkVersionsSnapshot> {
  return normalize(
    await invokeTauri<RawSnapshot>("compact_cowork_history", {
      repoAddress: input.repoAddress,
      folder: input.folder,
    }),
  );
}
