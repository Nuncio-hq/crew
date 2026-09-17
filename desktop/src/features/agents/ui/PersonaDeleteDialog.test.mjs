import assert from "node:assert/strict";
import test from "node:test";

import {
  personaDeleteDescription,
  personaDeleteTitle,
} from "./PersonaDeleteDialog.tsx";

// Regression guard for the persona-cascade consent copy: deleting a persona
// removes the persona definition, deletes linked instances, and archives each
// instance's identity on the relay (NIP-IA 9035), a durable externally visible
// side effect. The confirmation dialog must disclose those consequences before
// the destructive confirm.

const persona = { displayName: "Scout" };

test("delete title names the persona", () => {
  assert.equal(personaDeleteTitle(persona), "Delete Scout?");
  assert.equal(personaDeleteTitle(null), "Delete agent?");
});

test("cascade delete discloses relay archival (plural)", () => {
  const copy = personaDeleteDescription(persona, 3, 2);
  assert.match(copy, /Remove the Scout persona definition/);
  assert.match(copy, /Delete 3 linked agent instances/);
  assert.match(copy, /archive their identities on the relay/);
  assert.match(
    copy,
    /Messages, DM history, runtime installations, and worktrees remain in place/,
  );
  assert.match(
    copy,
    /Hermes profiles remain on this machine unless you explicitly choose Archive below/,
  );
});

test("cascade delete discloses relay archival (singular)", () => {
  const copy = personaDeleteDescription(persona, 1, 1);
  assert.match(copy, /Remove the Scout persona definition/);
  assert.match(copy, /Delete 1 linked agent instance/);
  assert.match(copy, /archive its identity on the relay/);
});

test("no instances keeps retained state and has no archival claim", () => {
  const copy = personaDeleteDescription(persona, 0);
  assert.match(copy, /Remove the Scout persona definition/);
  assert.match(
    copy,
    /Messages, DM history, runtime installations, and worktrees remain in place/,
  );
  assert.match(copy, /Hermes profile data is untouched/);
  assert.doesNotMatch(copy, /archiv/i);
});

test("null persona keeps the generic fallback", () => {
  assert.equal(personaDeleteDescription(null, 2), "Delete this agent.");
});
