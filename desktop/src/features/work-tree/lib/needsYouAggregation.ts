import {
  NEEDS_YOU_KIND_ORDER,
  type NeedsYouItem,
  type NeedsYouKind,
} from "./workTreeTypes";

const KIND_RANK: Record<NeedsYouKind, number> = {
  question: 3,
  approval: 2,
  evidence: 1,
};

function preferredKind(left: NeedsYouKind, right: NeedsYouKind): NeedsYouKind {
  return KIND_RANK[right] > KIND_RANK[left] ? right : left;
}

/**
 * Deduplicate by id. An item that matches several kinds is counted once
 * and kept on the highest-rank kind (question > approval > evidence).
 */
export function aggregateNeedsYou(items: readonly NeedsYouItem[]): {
  count: number;
  grouped: Record<NeedsYouKind, NeedsYouItem[]>;
  items: NeedsYouItem[];
} {
  const byId = new Map<string, NeedsYouItem>();
  for (const item of items) {
    const existing = byId.get(item.id);
    if (!existing) {
      byId.set(item.id, item);
      continue;
    }
    const kind = preferredKind(existing.kind, item.kind);
    byId.set(item.id, {
      ...existing,
      kind,
      title: existing.title || item.title,
    });
  }
  const unique = [...byId.values()].sort((left, right) => {
    const kind =
      KIND_RANK[right.kind] - KIND_RANK[left.kind] ||
      left.id.localeCompare(right.id);
    return kind;
  });
  const grouped: Record<NeedsYouKind, NeedsYouItem[]> = {
    approval: [],
    evidence: [],
    question: [],
  };
  for (const item of unique) {
    grouped[item.kind].push(item);
  }
  return { count: unique.length, grouped, items: unique };
}

export function needsYouSectionLabel(count: number): string {
  return `Needs you · ${count}`;
}

export function needsYouKindHeading(kind: NeedsYouKind): string {
  switch (kind) {
    case "question":
      return "Questions";
    case "approval":
      return "PR approvals";
    case "evidence":
      return "Evidence";
    default: {
      const _exhaustive: never = kind;
      return _exhaustive;
    }
  }
}

export { NEEDS_YOU_KIND_ORDER };
