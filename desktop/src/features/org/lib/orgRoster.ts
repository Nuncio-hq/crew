/** Founder-signed org roster projection (Crew). Relay is authoritative. */

export const ORG_ROSTER_D_TAG = "org";
export const CREW_HANDOFF_TAG = "crew-handoff";
export const CREW_BUDGET_TAG = "crew-budget";
export const CREW_BUDGET_STOP = "stop";

export type OrgBudget = {
  tokensPerDay: number;
  openWorkCap: number;
};

export type OrgNode = {
  agentPubkey: string;
  manager: string;
  domain: string;
  duties: string;
  cadence: string;
  budget: OrgBudget;
  officeChannel: string | null;
};

export type OrgRoster = {
  nodes: Record<string, OrgNode>;
  founderPubkey: string;
  eventId: string | null;
  createdAt: number | null;
};

export type OrgRosterError =
  | { kind: "json"; message: string }
  | { kind: "cycle"; agent: string }
  | { kind: "orphan"; agent: string; manager: string }
  | { kind: "self-manager"; agent: string }
  | { kind: "founder-as-node" }
  | { kind: "budget"; agent: string; manager: string }
  | { kind: "field"; message: string };

const HEX64 = /^[0-9a-f]{64}$/;

export function normalizeOrgPubkey(value: string): string | null {
  const trimmed = value.trim().toLowerCase();
  return HEX64.test(trimmed) ? trimmed : null;
}

export function parseOrgRoster(
  content: string,
  founderPubkey: string,
): { roster: OrgRoster } | { error: OrgRosterError } {
  const founder = normalizeOrgPubkey(founderPubkey);
  if (!founder) {
    return { error: { kind: "field", message: "invalid founder pubkey" } };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(content) as unknown;
  } catch (error) {
    return {
      error: {
        kind: "json",
        message: error instanceof Error ? error.message : "invalid JSON",
      },
    };
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { error: { kind: "json", message: "roster must be an object" } };
  }
  const rawNodes = (parsed as { nodes?: unknown }).nodes;
  if (!rawNodes || typeof rawNodes !== "object" || Array.isArray(rawNodes)) {
    return { error: { kind: "json", message: "nodes must be an object" } };
  }
  const nodes: Record<string, OrgNode> = {};
  for (const [rawAgent, rawNode] of Object.entries(
    rawNodes as Record<string, unknown>,
  )) {
    const agent = normalizeOrgPubkey(rawAgent);
    if (!agent) {
      return { error: { kind: "field", message: `invalid agent ${rawAgent}` } };
    }
    if (agent === founder) {
      return { error: { kind: "founder-as-node" } };
    }
    if (!rawNode || typeof rawNode !== "object" || Array.isArray(rawNode)) {
      return {
        error: { kind: "json", message: `node ${agent} is not an object` },
      };
    }
    const record = rawNode as Record<string, unknown>;
    const manager = normalizeOrgPubkey(String(record.manager ?? ""));
    if (!manager) {
      return {
        error: { kind: "field", message: `invalid manager for ${agent}` },
      };
    }
    if (manager === agent) {
      return { error: { kind: "self-manager", agent } };
    }
    const domain = String(record.domain ?? "").trim();
    if (!domain || domain.includes("\n") || domain.length > 128) {
      return {
        error: { kind: "field", message: "domain is required (one line)" },
      };
    }
    const budgetRaw = record.budget;
    if (
      !budgetRaw ||
      typeof budgetRaw !== "object" ||
      Array.isArray(budgetRaw)
    ) {
      return {
        error: { kind: "field", message: `budget missing for ${agent}` },
      };
    }
    const budgetRecord = budgetRaw as Record<string, unknown>;
    const tokensPerDay = Number(budgetRecord.tokens_per_day);
    const openWorkCap = Number(budgetRecord.open_work_cap);
    if (
      !Number.isFinite(tokensPerDay) ||
      !Number.isFinite(openWorkCap) ||
      tokensPerDay < 0 ||
      openWorkCap < 0
    ) {
      return {
        error: { kind: "field", message: `invalid budget for ${agent}` },
      };
    }
    const office =
      typeof record.office_channel === "string"
        ? record.office_channel.trim()
        : null;
    nodes[agent] = {
      agentPubkey: agent,
      manager,
      domain,
      duties: String(record.duties ?? "").trim(),
      cadence: String(record.cadence ?? "").trim(),
      budget: { tokensPerDay, openWorkCap },
      officeChannel: office || null,
    };
  }
  const roster: OrgRoster = {
    nodes,
    founderPubkey: founder,
    eventId: null,
    createdAt: null,
  };
  const treeError = validateOrgTree(roster);
  if (treeError) {
    return { error: treeError };
  }
  return { roster };
}

export function validateOrgTree(roster: OrgRoster): OrgRosterError | null {
  for (const node of Object.values(roster.nodes)) {
    if (node.manager !== roster.founderPubkey && !roster.nodes[node.manager]) {
      return { kind: "orphan", agent: node.agentPubkey, manager: node.manager };
    }
    const parent = roster.nodes[node.manager];
    if (
      parent &&
      (node.budget.tokensPerDay > parent.budget.tokensPerDay ||
        node.budget.openWorkCap > parent.budget.openWorkCap)
    ) {
      return { kind: "budget", agent: node.agentPubkey, manager: node.manager };
    }
    const seen = new Set<string>();
    let cursor = node.agentPubkey;
    while (cursor !== roster.founderPubkey) {
      if (seen.has(cursor)) {
        return { kind: "cycle", agent: cursor };
      }
      seen.add(cursor);
      const current = roster.nodes[cursor];
      if (!current) {
        break;
      }
      cursor = current.manager;
    }
  }
  return null;
}

export function serializeOrgRoster(roster: OrgRoster): string {
  const nodes: Record<string, unknown> = {};
  for (const [agent, node] of Object.entries(roster.nodes)) {
    nodes[agent] = {
      manager: node.manager,
      domain: node.domain,
      duties: node.duties,
      cadence: node.cadence,
      budget: {
        tokens_per_day: node.budget.tokensPerDay,
        open_work_cap: node.budget.openWorkCap,
      },
      ...(node.officeChannel ? { office_channel: node.officeChannel } : {}),
    };
  }
  return JSON.stringify({ nodes });
}

export function managerOf(roster: OrgRoster, agent: string): string | null {
  const key = normalizeOrgPubkey(agent);
  if (!key) {
    return null;
  }
  if (key === roster.founderPubkey) {
    return null;
  }
  return roster.nodes[key]?.manager ?? null;
}

export function isOfficer(roster: OrgRoster, agent: string): boolean {
  const key = normalizeOrgPubkey(agent);
  if (!key) {
    return false;
  }
  return Object.values(roster.nodes).some((node) => node.manager === key);
}

export function directReports(roster: OrgRoster, manager: string): OrgNode[] {
  const key = normalizeOrgPubkey(manager) ?? manager.trim().toLowerCase();
  return Object.values(roster.nodes).filter((node) => node.manager === key);
}

export function authorMayAssign(
  roster: OrgRoster,
  author: string,
  executor: string,
): boolean {
  const authorKey = normalizeOrgPubkey(author);
  const executorKey = normalizeOrgPubkey(executor);
  if (!authorKey || !executorKey) {
    return false;
  }
  if (authorKey === roster.founderPubkey) {
    return true;
  }
  const seen = new Set<string>();
  let cursor = executorKey;
  while (cursor !== roster.founderPubkey) {
    if (seen.has(cursor)) {
      return false;
    }
    seen.add(cursor);
    const node = roster.nodes[cursor];
    if (!node) {
      return false;
    }
    if (node.manager === authorKey) {
      return true;
    }
    cursor = node.manager;
  }
  return false;
}

export function portfolioCountForAgent(
  agent: string,
  repositories: readonly { maintainers?: readonly string[] }[],
): number {
  const key = normalizeOrgPubkey(agent);
  if (!key) {
    return 0;
  }
  return repositories.filter((repo) =>
    (repo.maintainers ?? []).some((value) => value.toLowerCase() === key),
  ).length;
}

export function displayNameForPubkey(
  pubkey: string,
  profiles: Record<
    string,
    { displayName?: string | null; name?: string | null }
  >,
): string {
  const key = pubkey.toLowerCase();
  const profile = profiles[key];
  return profile?.displayName?.trim() || profile?.name?.trim() || pubkey;
}

export function formatOrgError(error: OrgRosterError): string {
  switch (error.kind) {
    case "json":
    case "field":
      return error.message;
    case "cycle":
      return `Cycle involving ${error.agent}`;
    case "orphan":
      return `${error.agent} reports to unknown manager ${error.manager}`;
    case "self-manager":
      return `${error.agent} cannot manage itself`;
    case "founder-as-node":
      return "Founder cannot appear as a roster node";
    case "budget":
      return `Budget for ${error.agent} exceeds manager ${error.manager}`;
    default: {
      const exhaustive: never = error;
      return exhaustive;
    }
  }
}
