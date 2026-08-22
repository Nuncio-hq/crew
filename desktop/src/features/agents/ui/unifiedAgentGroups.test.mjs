import assert from "node:assert/strict";
import test from "node:test";

import { buildUnifiedGroups, pickProfileAgent } from "./unifiedAgentGroups.ts";

const NONE_ARCHIVED = () => false;

function agent(overrides = {}) {
  return {
    name: "Instance",
    pubkey: "a".repeat(64),
    personaId: null,
    status: "stopped",
    ...overrides,
  };
}

function persona(overrides = {}) {
  return { id: "persona-1", displayName: "Persona", ...overrides };
}

test("the shared profile target prefers the active persona instance", () => {
  const stopped = agent({
    name: "Earlier instance",
    pubkey: "a".repeat(64),
    status: "stopped",
  });
  const running = agent({
    name: "Current instance",
    pubkey: "b".repeat(64),
    status: "running",
  });

  assert.equal(pickProfileAgent([stopped, running], NONE_ARCHIVED), running);
  assert.equal(pickProfileAgent([running, stopped], NONE_ARCHIVED), running);
});

test("an archived instance early in file order cannot hijack the target", () => {
  const archived = agent({
    name: "Archived instance",
    pubkey: "a".repeat(64),
    status: "running",
  });
  const live = agent({
    name: "Live instance",
    pubkey: "b".repeat(64),
    status: "stopped",
  });
  const isArchived = (pubkey) => pubkey === archived.pubkey;

  // Archived is active AND first — without the filter it would win the sort.
  assert.equal(pickProfileAgent([archived, live], isArchived), live);
  assert.equal(pickProfileAgent([live, archived], isArchived), live);
});

test("all instances archived yields undefined for persona-only mode", () => {
  const first = agent({ pubkey: "a".repeat(64) });
  const second = agent({ pubkey: "b".repeat(64) });

  assert.equal(
    pickProfileAgent([first, second], () => true),
    undefined,
  );
});

test("a fail-open predicate keeps every instance eligible while loading", () => {
  const stopped = agent({ pubkey: "a".repeat(64), status: "stopped" });
  const running = agent({ pubkey: "b".repeat(64), status: "running" });

  // Fail-open (all false) during the archive-snapshot window: normal ranking.
  assert.equal(pickProfileAgent([stopped, running], NONE_ARCHIVED), running);
});

test("archived standalone custom agents are omitted while live peers remain", () => {
  const archived = agent({ pubkey: "a".repeat(64), personaId: null });
  const live = agent({ pubkey: "b".repeat(64), personaId: null });
  const isArchived = (pubkey) => pubkey === archived.pubkey;

  const { ungrouped } = buildUnifiedGroups([], [archived, live], isArchived);

  assert.deepEqual(
    ungrouped.map((entry) => entry.pubkey),
    [live.pubkey],
  );
});

test("archived unknown-persona agents are omitted while live peers remain", () => {
  const archived = agent({ pubkey: "a".repeat(64), personaId: "orphan" });
  const live = agent({ pubkey: "b".repeat(64), personaId: "orphan" });
  const isArchived = (pubkey) => pubkey === archived.pubkey;

  // No persona matches "orphan", so both land in the unknown bucket.
  const { unknown } = buildUnifiedGroups([], [archived, live], isArchived);

  assert.deepEqual(
    unknown.map((entry) => entry.pubkey),
    [live.pubkey],
  );
});

test("matched persona groups keep their full instance list including archived", () => {
  const archived = agent({ pubkey: "a".repeat(64), personaId: "persona-1" });
  const live = agent({ pubkey: "b".repeat(64), personaId: "persona-1" });
  const isArchived = (pubkey) => pubkey === archived.pubkey;

  // The card resolves its own target via pickProfileAgent; the group keeps the
  // archived record so an all-archived persona still forms a card in
  // persona-only mode rather than vanishing from the library.
  const { groups } = buildUnifiedGroups(
    [persona()],
    [archived, live],
    isArchived,
  );

  assert.equal(groups.length, 1);
  assert.deepEqual(
    groups[0].agents.map((entry) => entry.pubkey).sort(),
    [archived.pubkey, live.pubkey].sort(),
  );
});

test("a fail-open predicate keeps every standalone agent discoverable", () => {
  const first = agent({ pubkey: "a".repeat(64), personaId: null });
  const second = agent({ pubkey: "b".repeat(64), personaId: null });

  const { ungrouped } = buildUnifiedGroups([], [first, second], NONE_ARCHIVED);

  assert.equal(ungrouped.length, 2);
});
