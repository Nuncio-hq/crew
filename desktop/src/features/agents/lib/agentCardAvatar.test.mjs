import assert from "node:assert/strict";
import test from "node:test";

import {
  isAgentCardAvatarLoading,
  personaAvatarById,
  resolveAgentCardAvatarUrl,
} from "./agentCardAvatar.ts";

test("running agent card prefers the pubkey profile avatar", () => {
  assert.equal(
    resolveAgentCardAvatarUrl(
      "https://relay.example/instance.png",
      "https://relay.example/definition.png",
    ),
    "https://relay.example/instance.png",
  );
});

test("running agent card falls back to the definition avatar", () => {
  assert.equal(
    resolveAgentCardAvatarUrl(null, " https://relay.example/definition.png "),
    "https://relay.example/definition.png",
  );
});

test("running agent card ignores blank avatar values", () => {
  assert.equal(resolveAgentCardAvatarUrl("  ", ""), null);
});

test("later candidates fill in when earlier avatars are blank", () => {
  assert.equal(
    resolveAgentCardAvatarUrl(null, "  ", "https://relay.example/persona.png"),
    "https://relay.example/persona.png",
  );
});

test("linked agent actions wait for the authoritative profile avatar", () => {
  assert.equal(isAgentCardAvatarLoading(true, true), true);
  assert.equal(isAgentCardAvatarLoading(true, false), false);
});

test("unlinked persona actions do not wait for a profile", () => {
  assert.equal(isAgentCardAvatarLoading(false, true), false);
});

test("personaAvatarById indexes default avatars by persona id", () => {
  assert.deepEqual(
    personaAvatarById([
      { id: "fizz", avatarUrl: "https://example.com/fizz.png" },
      { id: "honey", avatarUrl: null },
    ]),
    {
      fizz: "https://example.com/fizz.png",
      honey: null,
    },
  );
});
