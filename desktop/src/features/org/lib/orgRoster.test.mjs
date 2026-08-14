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

const mockOwner = "deadbeef".repeat(8);
const tyler =
  "e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34";
const hermes =
  "953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f";
const mismatched = parseOrgRoster(
  JSON.stringify({
    nodes: {
      [hermes]: {
        manager: mockOwner,
        domain: "office",
        duties: "survey",
        cadence: "weekly",
        budget: { tokens_per_day: 100, open_work_cap: 2 },
      },
    },
  }),
  tyler,
);
if (!("error" in mismatched) || mismatched.error.kind !== "orphan") {
  throw new Error(
    "Hermes reporting to deadbeef must be an orphan when tyler signed",
  );
}
const aligned = parseOrgRoster(
  JSON.stringify({
    nodes: {
      [hermes]: {
        manager: mockOwner,
        domain: "office",
        duties: "survey",
        cadence: "weekly",
        budget: { tokens_per_day: 100, open_work_cap: 2 },
      },
    },
  }),
  mockOwner,
);
if ("error" in aligned) {
  throw new Error(aligned.error.kind);
}
