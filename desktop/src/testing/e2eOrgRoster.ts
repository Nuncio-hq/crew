import { orgRosterQueryKey } from "@/features/org/hooks/useOrgRosterQuery";
import { setOrgRosterProjection } from "@/features/org/lib/orgProjectionStore";
import { parseOrgRoster } from "@/features/org/lib/orgRoster";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_ORG_ROSTER } from "@/shared/constants/kinds";

type MockFilter = {
  kinds?: number[];
  "#d"?: string[];
  authors?: string[];
};

let mockOrgRosterEvent: RelayEvent | null = null;

function mockEventId(seed: string): string {
  const hex = seed
    .replace(/[^0-9a-f]/gi, "a")
    .toLowerCase()
    .padEnd(64, "a");
  return hex.slice(0, 64);
}

export function resetE2eOrgRoster(): void {
  mockOrgRosterEvent = null;
  setOrgRosterProjection(null);
}

export function setE2eOrgRoster(input: {
  content: string;
  pubkey: string;
  createdAt?: number;
  id?: string;
}): RelayEvent {
  const event: RelayEvent = {
    id: input.id ?? mockEventId(`org${input.createdAt ?? 1}`),
    pubkey: input.pubkey,
    created_at: input.createdAt ?? Math.floor(Date.now() / 1000),
    kind: KIND_ORG_ROSTER,
    tags: [["d", "org"]],
    content: input.content,
    sig: `mocksig${"0".repeat(121)}`.slice(0, 128),
  };
  mockOrgRosterEvent = event;
  projectOrgRosterEvent(event);
  return event;
}

export function acceptPublishedOrgRoster(event: RelayEvent): boolean {
  if (event.kind !== KIND_ORG_ROSTER) {
    return false;
  }
  mockOrgRosterEvent = event;
  projectOrgRosterEvent(event);
  return true;
}

export function filterE2eOrgRosterEvents(filter: MockFilter): RelayEvent[] {
  if (!filter.kinds?.includes(KIND_ORG_ROSTER)) {
    return [];
  }
  if (!mockOrgRosterEvent) {
    return [];
  }
  const dTags = filter["#d"];
  if (dTags && !dTags.includes("org")) {
    return [];
  }
  const authors = filter.authors?.map((author) => author.toLowerCase());
  if (authors && !authors.includes(mockOrgRosterEvent.pubkey.toLowerCase())) {
    return [];
  }
  return [mockOrgRosterEvent];
}

export function isOrgRosterKind(kind: number): boolean {
  return kind === KIND_ORG_ROSTER;
}

function projectOrgRosterEvent(event: RelayEvent): void {
  const parsed = parseOrgRoster(event.content, event.pubkey);
  if ("error" in parsed) {
    setOrgRosterProjection(null);
  } else {
    setOrgRosterProjection({
      ...parsed.roster,
      eventId: event.id,
      createdAt: event.created_at,
    });
  }
  void window.__BUZZ_E2E_QUERY_CLIENT__?.invalidateQueries({
    queryKey: orgRosterQueryKey,
  });
}
