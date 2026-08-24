import assert from "node:assert/strict";
import test from "node:test";

import { resolveAgentCardModelLabel } from "./agentCardModelLabel.ts";

test("resolveAgentCardModelLabel — unspawned definition with explicit model renders the model, not inherited", () => {
  const label = resolveAgentCardModelLabel({
    agent: undefined,
    personaModel: "gpt-5",
    defaultModel: "claude-sonnet",
  });
  assert.equal(label, "gpt-5");
});

test("resolveAgentCardModelLabel — unspawned definition with no model renders the default", () => {
  const label = resolveAgentCardModelLabel({
    agent: undefined,
    personaModel: null,
    defaultModel: "claude-sonnet",
  });
  assert.equal(label, "Default model (claude-sonnet)");
});

test("resolveAgentCardModelLabel — linked instance inheriting the global default ignores stale persona.model", () => {
  const label = resolveAgentCardModelLabel({
    agent: { modelSource: "global", model: "stale-model" },
    personaModel: "gpt-5",
    defaultModel: "claude-sonnet",
  });
  assert.equal(label, "Default model (claude-sonnet)");
});

test("resolveAgentCardModelLabel — linked instance with no modelSource (legacy/unset) is treated as inherited", () => {
  const label = resolveAgentCardModelLabel({
    agent: { modelSource: null, model: "stale-model" },
    personaModel: "gpt-5",
    defaultModel: "claude-sonnet",
  });
  assert.equal(label, "Default model (claude-sonnet)");
});

test("resolveAgentCardModelLabel — linked instance with an explicit resolved model renders that model", () => {
  const label = resolveAgentCardModelLabel({
    agent: { modelSource: "definition", model: "gpt-5" },
    personaModel: "should-not-be-used",
    defaultModel: "claude-sonnet",
  });
  assert.equal(label, "gpt-5");
});

test("resolveAgentCardModelLabel — non-inherited agent with a blank resolved model falls back to the default", () => {
  const label = resolveAgentCardModelLabel({
    agent: { modelSource: "instance_legacy", model: "  " },
    personaModel: null,
    defaultModel: "claude-sonnet",
  });
  assert.equal(label, "Default model (claude-sonnet)");
});

test("resolveAgentCardModelLabel — Hermes profile binding shows profile label, not Crew default", () => {
  const label = resolveAgentCardModelLabel({
    agent: {
      modelSource: null,
      model: null,
      hermesProfile: "default",
    },
    personaModel: null,
    defaultModel: "claude-opus-4-20250514",
  });
  assert.equal(label, "Profile: Personal (default)");
});

test("resolveAgentCardModelLabel — Hermes profile with resolved model shows both", () => {
  const label = resolveAgentCardModelLabel({
    agent: {
      modelSource: null,
      model: "gpt-5",
      hermesProfile: "scout",
    },
    personaModel: null,
    defaultModel: "claude-opus-4-20250514",
  });
  assert.equal(label, "scout · gpt-5");
});

test("resolveAgentCardModelLabel — profileModelFromDisk overrides stale agent model", () => {
  const label = resolveAgentCardModelLabel({
    agent: {
      modelSource: "global",
      model: "claude-opus-4-20250514",
      hermesProfile: "default",
    },
    personaModel: null,
    defaultModel: "claude-opus-4-20250514",
    profileModelFromDisk: "grok-3",
  });
  assert.equal(label, "Personal (default) · grok-3");
});

test("resolveAgentCardModelLabel — profileModelFromDisk overrides stale agent model for scout", () => {
  const label = resolveAgentCardModelLabel({
    agent: {
      modelSource: "global",
      model: "claude-opus-4-20250514",
      hermesProfile: "scout",
    },
    personaModel: null,
    defaultModel: "claude-opus-4-20250514",
    profileModelFromDisk: "claude-sonnet",
  });
  assert.equal(label, "scout · claude-sonnet");
});
