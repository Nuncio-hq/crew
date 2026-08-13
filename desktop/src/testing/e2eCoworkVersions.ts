/**
 * In-memory Cowork Versions snapshot for the e2e mock bridge.
 *
 * Seed via `window.__BUZZ_E2E_COWORK_VERSIONS__` in addInitScript before the
 * bridge boots. Restore mutates the snapshot so the Versions timeline can
 * show the checkpoint-before-restore entry.
 */

export type E2eCoworkVersionKind = "baseline" | "external" | "turn" | "restore";

export type E2eCoworkVersionEntry = {
  id: string;
  kind: E2eCoworkVersionKind;
  summary: string;
  timestamp: number;
  agentName: string | null;
  threadTitle: string | null;
  threadId: string | null;
  filesChanged: string[];
};

export type E2eCoworkVersionsSnapshot = {
  versions: E2eCoworkVersionEntry[];
  excluded: Array<{ path: string; sizeBytes: number }>;
  notice: string | null;
  rebuilt: boolean;
  sizeThresholdBytes: number;
};

const COWORK_COMMANDS = new Set([
  "init_cowork_history",
  "list_cowork_versions",
  "restore_cowork_file",
  "restore_cowork_folder",
  "compact_cowork_history",
]);

export function isCoworkVersionsCommand(command: string): boolean {
  return COWORK_COMMANDS.has(command);
}

export function defaultCoworkVersionsSnapshot(): E2eCoworkVersionsSnapshot {
  const now = Math.floor(Date.now() / 1000);
  return {
    versions: [
      {
        id: "turn1",
        kind: "turn",
        summary: "Turn 1 — Hermes · thread 'Q3 proposal'",
        timestamp: now,
        agentName: "Hermes",
        threadTitle: "Q3 proposal",
        threadId: "c".repeat(64),
        filesChanged: ["proposal.docx"],
      },
      {
        id: "ext1",
        kind: "external",
        summary: "External changes",
        timestamp: now - 120,
        agentName: null,
        threadTitle: null,
        threadId: null,
        filesChanged: ["notes.txt"],
      },
    ],
    excluded: [{ path: "deck.pptx", sizeBytes: 62_914_560 }],
    notice: null,
    rebuilt: false,
    sizeThresholdBytes: 52_428_800,
  };
}

function snapshotFromWindow(): E2eCoworkVersionsSnapshot {
  const seeded = (
    globalThis as typeof globalThis & {
      __BUZZ_E2E_COWORK_VERSIONS__?: E2eCoworkVersionsSnapshot;
    }
  ).__BUZZ_E2E_COWORK_VERSIONS__;
  return seeded ?? defaultCoworkVersionsSnapshot();
}

function writeSnapshot(
  next: E2eCoworkVersionsSnapshot,
): E2eCoworkVersionsSnapshot {
  (
    globalThis as typeof globalThis & {
      __BUZZ_E2E_COWORK_VERSIONS__?: E2eCoworkVersionsSnapshot;
    }
  ).__BUZZ_E2E_COWORK_VERSIONS__ = next;
  return next;
}

function prependRestore(
  current: E2eCoworkVersionsSnapshot,
  filesChanged: string[],
): E2eCoworkVersionsSnapshot {
  return {
    ...current,
    versions: [
      {
        id: `restore-${current.versions.length + 1}`,
        kind: "restore",
        summary: "Restored version",
        timestamp: Math.floor(Date.now() / 1000),
        agentName: "you",
        threadTitle: null,
        threadId: null,
        filesChanged,
      },
      ...current.versions,
    ],
  };
}

export function handleCoworkVersionsCommand(
  command: string,
  payload: unknown,
): E2eCoworkVersionsSnapshot {
  const current = snapshotFromWindow();
  const input = (payload ?? {}) as {
    relativePath?: string;
    relative_path?: string;
    commit?: string;
  };
  switch (command) {
    case "init_cowork_history":
    case "list_cowork_versions":
    case "compact_cowork_history":
      return writeSnapshot(current);
    case "restore_cowork_file": {
      const path = input.relativePath ?? input.relative_path ?? "proposal.docx";
      return writeSnapshot(prependRestore(current, [path]));
    }
    case "restore_cowork_folder":
      return writeSnapshot(prependRestore(current, []));
    default:
      return writeSnapshot(current);
  }
}
