import assert from "node:assert/strict";
import test from "node:test";

import {
  isAgentCardAvatarLoading,
  personaAvatarById,
  personaRuntimeById,
  resolveAgentCardAvatarUrl,
  resolveManagedAgentDisplayAvatarUrl,
  resolveRuntimeDefaultAvatarUrl,
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

test("personaRuntimeById indexes harness ids by persona id", () => {
  assert.deepEqual(
    personaRuntimeById([
      { id: "fizz", runtime: "hermes" },
      { id: "honey", runtime: null },
    ]),
    {
      fizz: "hermes",
      honey: null,
    },
  );
});

test("resolveRuntimeDefaultAvatarUrl covers compiled-in runtimes and aliases", () => {
  assert.equal(
    resolveRuntimeDefaultAvatarUrl("hermes"),
    "/harness-logos/hermes.png",
  );
  assert.equal(
    resolveRuntimeDefaultAvatarUrl("hermes-agent"),
    "/harness-logos/hermes.png",
  );
  assert.equal(
    resolveRuntimeDefaultAvatarUrl("claude"),
    "/harness-logos/claude.png",
  );
  assert.equal(
    resolveRuntimeDefaultAvatarUrl("claude-code"),
    "/harness-logos/claude.png",
  );
  assert.equal(
    resolveRuntimeDefaultAvatarUrl("goose"),
    "/harness-logos/goose.svg",
  );
  assert.equal(
    resolveRuntimeDefaultAvatarUrl("cursor"),
    "/harness-logos/cursor.svg",
  );
  assert.equal(
    resolveRuntimeDefaultAvatarUrl("codex"),
    "/harness-logos/terminal.svg",
  );
  assert.equal(resolveRuntimeDefaultAvatarUrl("custom"), null);
  assert.equal(resolveRuntimeDefaultAvatarUrl("unknown-runtime"), null);
});

test("managed agent display avatar uses Hermes runtime when no picture exists", () => {
  assert.equal(
    resolveManagedAgentDisplayAvatarUrl({
      profileAvatarUrl: null,
      agentAvatarUrl: null,
      agentRuntime: "hermes",
      personaAvatarUrl: null,
      personaRuntime: null,
    }),
    "/harness-logos/hermes.png",
  );
});

test("managed agent display avatar prefers persona clay-bee over runtime", () => {
  assert.equal(
    resolveManagedAgentDisplayAvatarUrl({
      profileAvatarUrl: null,
      agentAvatarUrl: null,
      agentRuntime: "hermes",
      personaAvatarUrl: "data:image/png;base64,fizz",
      personaRuntime: "hermes",
    }),
    "data:image/png;base64,fizz",
  );
});

test("managed agent display avatar inherits persona runtime when instance is unpinned", () => {
  assert.equal(
    resolveManagedAgentDisplayAvatarUrl({
      profileAvatarUrl: null,
      agentAvatarUrl: null,
      agentRuntime: null,
      personaAvatarUrl: null,
      personaRuntime: "hermes",
    }),
    "/harness-logos/hermes.png",
  );
});

test("managed agent display avatar uses Claude, Goose, Cursor, and Codex faces", () => {
  assert.equal(
    resolveManagedAgentDisplayAvatarUrl({ agentRuntime: "claude" }),
    "/harness-logos/claude.png",
  );
  assert.equal(
    resolveManagedAgentDisplayAvatarUrl({ agentRuntime: "goose" }),
    "/harness-logos/goose.svg",
  );
  assert.equal(
    resolveManagedAgentDisplayAvatarUrl({ agentRuntime: "cursor" }),
    "/harness-logos/cursor.svg",
  );
  assert.equal(
    resolveManagedAgentDisplayAvatarUrl({ agentRuntime: "codex" }),
    "/harness-logos/terminal.svg",
  );
});
