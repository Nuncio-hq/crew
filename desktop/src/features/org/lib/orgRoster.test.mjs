import {
  authorMayAssign,
  parseOrgRoster,
  serializeOrgRoster,
  validateOrgTree,
} from "./orgRoster.ts";

const founder = "aa".repeat(32);
const officer = "bb".repeat(32);
const ic = "cc".repeat(32);
const peer = "dd".repeat(32);

const json = JSON.stringify({
  nodes: {
    [officer]: {
      manager: founder,
      domain: "office",
      duties: "survey",
      cadence: "weekly",
      budget: { tokens_per_day: 100, open_work_cap: 2 },
    },
    [ic]: {
      manager: officer,
      domain: "execution",
      duties: "build",
      cadence: "daily",
      budget: { tokens_per_day: 40, open_work_cap: 1 },
    },
    [peer]: {
      manager: founder,
      domain: "peer",
      duties: "other",
      cadence: "weekly",
      budget: { tokens_per_day: 80, open_work_cap: 1 },
    },
  },
});

const parsed = parseOrgRoster(json, founder);
if ("error" in parsed) {
  throw new Error(parsed.error.kind);
}
const { roster } = parsed;
if (!authorMayAssign(roster, founder, ic)) {
  throw new Error("founder skip-level must assign");
}
if (!authorMayAssign(roster, officer, ic)) {
  throw new Error("manager must assign");
}
if (authorMayAssign(roster, peer, ic)) {
  throw new Error("peer must not auto-create work");
}

const overflow = structuredClone(roster);
overflow.nodes[ic].budget.tokensPerDay = 200;
if (validateOrgTree(overflow)?.kind !== "budget") {
  throw new Error("child budget must not exceed parent");
}

const roundTrip = parseOrgRoster(serializeOrgRoster(roster), founder);
if ("error" in roundTrip) {
  throw new Error(roundTrip.error.kind);
}
