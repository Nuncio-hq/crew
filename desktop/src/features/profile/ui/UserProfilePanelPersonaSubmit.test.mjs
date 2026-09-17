import assert from "node:assert/strict";
import test from "node:test";

import {
  submitProfilePersonaDialog,
  validateLinkedAgentRuntimeEdit,
} from "./UserProfilePanelPersonaSubmit.ts";

function agent(overrides = {}) {
  return {
    pubkey: "deadbeef".repeat(8),
    name: "Fizz",
    personaId: "persona-1",
    relayUrl: "ws://localhost:3000",
    acpCommand: "buzz-acp",
    agentCommand: "goose",
    agentArgs: [],
    mcpCommand: "",
    turnTimeoutSeconds: 320,
    idleTimeoutSeconds: null,
    maxTurnDurationSeconds: null,
    parallelism: 1,
    systemPrompt: "Prompt",
    avatarUrl: null,
    model: null,
    envVars: {},
    status: "stopped",
    pid: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    lastStartedAt: null,
    lastStoppedAt: null,
    lastExitCode: null,
    lastError: null,
    logPath: null,
    startOnAppLaunch: true,
    backend: { type: "local" },
    backendAgentId: null,
    respondTo: "owner-only",
    respondToAllowlist: [],
    hermesProfile: null,
    ...overrides,
  };
}

function persona(overrides = {}) {
  return {
    id: "persona-1",
    displayName: "Fizz",
    avatarUrl: null,
    systemPrompt: "Prompt",
    runtime: "goose",
    model: null,
    provider: null,
    namePool: [],
    isBuiltIn: false,
    isActive: true,
    envVars: {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function updateInput(overrides = {}) {
  return {
    id: "persona-1",
    displayName: "Fizz",
    avatarUrl: undefined,
    systemPrompt: "Prompt",
    runtime: "claude",
    model: undefined,
    provider: undefined,
    namePool: [],
    ...overrides,
  };
}

function runtime(overrides = {}) {
  return {
    id: "claude",
    label: "Claude Code",
    avatarUrl: "",
    availability: "available",
    command: "claude",
    binaryPath: "/usr/local/bin/claude",
    defaultArgs: [],
    mcpCommand: null,
    installHint: "",
    installInstructionsUrl: "",
    canAutoInstall: false,
    underlyingCliPath: null,
    ...overrides,
  };
}

test("validateLinkedAgentRuntimeEdit allows available runtime changes", () => {
  assert.equal(
    validateLinkedAgentRuntimeEdit({
      input: updateInput({ runtime: "claude" }),
      managedAgent: agent(),
      previousPersona: persona({ runtime: "goose" }),
      runtimes: [runtime()],
    }),
    null,
  );
});

test("validateLinkedAgentRuntimeEdit rejects unavailable linked-agent runtime changes", () => {
  assert.equal(
    validateLinkedAgentRuntimeEdit({
      input: updateInput({ runtime: "claude" }),
      managedAgent: agent(),
      previousPersona: persona({ runtime: "goose" }),
      runtimes: [runtime({ availability: "cli_missing", command: null })],
    }),
    "Claude Code is not available. Install it before saving this linked agent.",
  );
});

test("validateLinkedAgentRuntimeEdit allows unchanged or unlinked runtime preferences", () => {
  assert.equal(
    validateLinkedAgentRuntimeEdit({
      input: updateInput({ runtime: "goose" }),
      managedAgent: agent(),
      previousPersona: persona({ runtime: "goose" }),
      runtimes: [],
    }),
    null,
  );

  assert.equal(
    validateLinkedAgentRuntimeEdit({
      input: updateInput({ runtime: "claude" }),
      managedAgent: undefined,
      previousPersona: persona({ runtime: "goose" }),
      runtimes: [],
    }),
    null,
  );
});

test("profile-panel definition save forwards an explicit Hermes binding", async () => {
  const updates = [];
  let done = false;

  await submitProfilePersonaDialog({
    createManagedAgentForPersona: async () => {
      throw new Error("unexpected create");
    },
    createPersona: async () => persona(),
    input: updateInput({ runtime: "hermes" }),
    managedAgent: agent({ agentCommand: "hermes-acp" }),
    onDone: () => {
      done = true;
    },
    options: { hermesProfile: "crew353-keep-20260915" },
    previousPersona: persona({ runtime: "hermes" }),
    runtimes: [],
    updateManagedAgent: async (input) => {
      updates.push(input);
      return {
        agent: agent({ hermesProfile: input.hermesProfile }),
        profileSyncError: null,
      };
    },
    updatePersona: async () => persona({ runtime: "hermes" }),
  });

  assert.deepEqual(updates, [
    {
      pubkey: "deadbeef".repeat(8),
      hermesProfile: "crew353-keep-20260915",
    },
  ]);
  assert.equal(done, true);
});
