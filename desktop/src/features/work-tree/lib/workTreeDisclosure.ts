import type { WorkTreeDisclosure } from "./workTreeTypes";

const STORAGE_KEY = "crew.work-tree.disclosure.v1";

type DisclosureMap = Record<string, WorkTreeDisclosure>;

const listeners = new Set<() => void>();
let cache: DisclosureMap | null = null;

function read(): DisclosureMap {
  if (cache) return cache;
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (!raw) {
      cache = {};
      return cache;
    }
    const parsed: unknown = JSON.parse(raw);
    cache =
      parsed && typeof parsed === "object" && !Array.isArray(parsed)
        ? (parsed as DisclosureMap)
        : {};
  } catch {
    cache = {};
  }
  return cache;
}

function write(next: DisclosureMap): void {
  cache = next;
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Private mode / quota — in-memory still wins for the session.
  }
  for (const listener of listeners) listener();
}

export function getWorkTreeDisclosure(
  channelId: string,
): WorkTreeDisclosure | undefined {
  return read()[channelId];
}

export function setWorkTreeDisclosure(
  channelId: string,
  patch: WorkTreeDisclosure,
): void {
  const current = read();
  write({
    ...current,
    [channelId]: { ...current[channelId], ...patch },
  });
}

export function subscribeWorkTreeDisclosure(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetWorkTreeDisclosureStore(): void {
  cache = {};
  try {
    globalThis.localStorage?.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
  for (const listener of listeners) listener();
}
