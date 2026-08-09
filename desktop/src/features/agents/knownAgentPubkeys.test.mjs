import assert from "node:assert/strict";
import test from "node:test";

import {
  mergeKnownAgentPubkeys,
  mergeOwnedAgentPubkeys,
} from "./knownAgentPubkeys.ts";

const MANAGED =
  "1111111111111111111111111111111111111111111111111111111111111111";
const RELAY =
  "2222222222222222222222222222222222222222222222222222222222222222";

test("mergesTrustedSources", () => {
  const merged = mergeKnownAgentPubkeys(
    [{ pubkey: MANAGED }],
    [{ pubkey: RELAY }],
  );

  assert.deepEqual([...merged].sort(), [MANAGED, RELAY].sort());
});

test("undefinedSources_yieldEmptySet", () => {
  const merged = mergeKnownAgentPubkeys(undefined, undefined);

  assert.equal(merged.size, 0);
});

test("normalisesCaseAndWhitespace_dedupingAcrossSources", () => {
  // The same agent appearing in multiple sources with different casing /
  // stray whitespace must collapse to one normalised entry, so membership
  // checks against normalizePubkey output always hit.
  const merged = mergeKnownAgentPubkeys(
    [{ pubkey: MANAGED.toUpperCase() }],
    [{ pubkey: ` ${MANAGED}` }],
  );

  assert.deepEqual([...merged], [MANAGED]);
});

test("owned agents include only profile-verified agents", () => {
  const merged = mergeOwnedAgentPubkeys(
    {
      [MANAGED]: { ownerPubkey: " owner " },
      [RELAY]: { ownerPubkey: " owner " },
      other: { ownerPubkey: "somebody-else" },
    },
    "OWNER",
  );

  assert.deepEqual([...merged].sort(), [MANAGED, RELAY].sort());
});

test("owned agents exclude unverified managed records after identity switch", () => {
  const merged = mergeOwnedAgentPubkeys(
    { [MANAGED]: { ownerPubkey: "owner-a" } },
    "owner-b",
  );

  assert.equal(merged.size, 0);
});

test("owned agents exclude agents controlled by somebody else", () => {
  const merged = mergeOwnedAgentPubkeys(
    { [RELAY]: { ownerPubkey: "somebody-else" } },
    "owner",
  );

  assert.equal(merged.size, 0);
});
