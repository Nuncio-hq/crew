import type { OrgRoster } from "./orgRoster";

let cached: OrgRoster | null = null;
const listeners = new Set<() => void>();

export function getOrgRosterProjection(): OrgRoster | null {
  return cached;
}

export function setOrgRosterProjection(next: OrgRoster | null): void {
  cached = next;
  for (const listener of listeners) {
    listener();
  }
}

export function subscribeOrgRosterProjection(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetOrgRosterProjection(): void {
  setOrgRosterProjection(null);
}
