import assert from "node:assert/strict";
import test from "node:test";
import { npubEncode } from "nostr-tools/nip19";
import {
  createRoleDraft,
  serializeRoleDraft,
  removeRole,
} from "./channelRoleDraft.ts";

const one = "a".repeat(64),
  two = "b".repeat(64);
function canvas() {
  return {
    definitions: [
      { roleLabel: "Review", definition: " Inspect exactly. " },
      { roleLabel: "Research", definition: "Read" },
    ],
    storedAssignments: {
      [one]: "Review",
      [two]: "Review",
      " bad-key ": " Ghost ",
    },
    storedRouting: { audit: "Review" },
    storedCapabilities: { review: ["buzz-dev-mcp"] },
    contactPubkey: one,
  };
}

test("role form retains unassigned definitions, shared holders and unresolved raw entries", () => {
  const draft = createRoleDraft(canvas(), [one, two]);
  assert.equal(draft.roles.length, 2);
  assert.equal(draft.assignments[one], draft.roles[0].id);
  assert.equal(draft.assignments[two], draft.roles[0].id);
  assert.deepEqual(draft.preservedAssignments, { " bad-key ": " Ghost " });
  const saved = serializeRoleDraft(draft);
  assert.equal(saved.definitions[0].definition, " Inspect exactly. ");
  assert.equal(saved.contact, one);
});

test("opening and saving preserves a resolved npub source spelling", () => {
  const raw = `  ${npubEncode(one)}  `;
  const source = canvas();
  source.storedAssignments = { [raw]: "Review" };
  const draft = createRoleDraft(source, [one]);

  assert.equal(draft.assignments[one], draft.roles[0].id);
  const saved = serializeRoleDraft(draft);
  assert.deepEqual(saved.assignments, {});
  assert.deepEqual(saved.preserved_assignments, { [raw]: "Review" });
});

test("editing a resolved assignment emits the canonical key", () => {
  const raw = `  ${npubEncode(one)}  `;
  const source = canvas();
  source.storedAssignments = { [raw]: "Review" };
  const draft = createRoleDraft(source, [one]);
  draft.assignments[one] = draft.roles[1].id;

  const saved = serializeRoleDraft(draft);
  assert.deepEqual(saved.assignments, { [one]: "Research" });
  assert.deepEqual(saved.preserved_assignments, {});
});

test("renaming a resolved role keeps the source assignment spelling", () => {
  const raw = `  ${npubEncode(one)}  `;
  const source = canvas();
  source.storedAssignments = { [raw]: "Review" };
  const draft = createRoleDraft(source, [one]);
  draft.roles[0].label = "Inspection";

  const saved = serializeRoleDraft(draft);
  assert.deepEqual(saved.assignments, {});
  assert.deepEqual(saved.preserved_assignments, { [raw]: "Review" });
  assert.deepEqual(saved.renames, { Review: "Inspection" });
});

test("role form moves an agent once and records rename without losing definition meaning", () => {
  const draft = createRoleDraft(canvas(), [one, two]);
  assert.equal(draft.roles.length, 2);
  draft.assignments[one] = draft.roles[1].id;
  draft.roles[0].label = "Code Review";
  const saved = serializeRoleDraft(draft);
  assert.deepEqual(saved.assignments, {
    [one]: "Research",
    [two]: "Code Review",
  });
  assert.deepEqual(saved.renames, { Review: "Code Review" });
});

test("role removal requires explicit holder and reference resolution", () => {
  const draft = createRoleDraft(canvas(), [one, two]);
  assert.equal(draft.roles.length, 2);
  assert.throws(() => removeRole(draft, draft.roles[0].id, false), /resolve/i);
  const saved = serializeRoleDraft(removeRole(draft, draft.roles[0].id, true));
  assert.deepEqual(saved.assignments, {});
  assert.deepEqual(saved.remove_routing, ["audit"]);
  assert.deepEqual(saved.remove_capabilities, ["review"]);
  assert.equal(saved.definitions.length, 1);
  assert.deepEqual(saved.preserved_assignments, { " bad-key ": " Ghost " });
});

test("role rename preserves ordinary labels that match object property names", () => {
  const source = canvas();
  source.definitions = [
    { roleLabel: "__proto__", definition: "Keep this role" },
  ];
  const draft = createRoleDraft(source, []);
  draft.roles[0].label = "Review";
  assert.equal(
    Object.hasOwn(serializeRoleDraft(draft).renames, "__proto__"),
    true,
  );
});
