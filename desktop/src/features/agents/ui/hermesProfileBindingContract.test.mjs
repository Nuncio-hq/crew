/**
 * Validation helpers for Hermes profile binding (Phase 02B).
 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  hermesProfileBindingError,
  profileOwnedModelLabel,
  runtimeOffersProfileBinding,
  runtimeOwnsModelViaProfile,
  validateHermesProfileName,
} from "../lib/hermesProfileBinding.ts";
import {
  isModelOwnedByProfile,
  deriveAgentConfigFieldModel,
} from "../lib/agentConfigCore.ts";

test("validateHermesProfileName rejects default and bad shapes", () => {
  assert.match(validateHermesProfileName("default") ?? "", /default/);
  assert.match(validateHermesProfileName("Bad Name") ?? "", /lowercase/);
  assert.equal(validateHermesProfileName("scout"), null);
  assert.equal(validateHermesProfileName(""), null);
});

test("hermesProfileBindingError requires a name when required", () => {
  assert.match(hermesProfileBindingError("", true) ?? "", /Bind a Hermes/);
  assert.equal(hermesProfileBindingError("", false), null);
  assert.match(hermesProfileBindingError("default", true) ?? "", /default/);
});

test("runtimeOwnsModelViaProfile needs profileArg + providerLocked + no modelEnvVar", () => {
  assert.equal(
    runtimeOwnsModelViaProfile({
      profileArg: "-p",
      providerLocked: true,
      modelEnvVar: null,
    }),
    true,
  );
  assert.equal(
    runtimeOwnsModelViaProfile({
      profileArg: "-p",
      providerLocked: false,
      modelEnvVar: null,
    }),
    false,
  );
  assert.equal(
    runtimeOwnsModelViaProfile({
      profileArg: null,
      providerLocked: true,
      modelEnvVar: null,
    }),
    false,
  );
  assert.equal(runtimeOffersProfileBinding({ profileArg: "-p" }), true);
  assert.equal(runtimeOffersProfileBinding({ profileArg: null }), false);
});

test("profileOwnedModelLabel formats C-04 copy", () => {
  assert.equal(
    profileOwnedModelLabel("scout", "gpt-4.1"),
    "Model: decided by profile scout — currently gpt-4.1",
  );
  assert.equal(
    profileOwnedModelLabel("scout", null),
    "Model: decided by profile scout",
  );
  assert.equal(profileOwnedModelLabel(null, null), "Model: decided by profile");
});

test("isModelOwnedByProfile reads the named omission", () => {
  const model = deriveAgentConfigFieldModel({
    config: {
      env_vars: {},
      model: null,
      preferred_runtime: null,
      provider: null,
    },
    hermesProfile: "scout",
    runtime: {
      id: "hermes",
      label: "Hermes",
      avatarUrl: "",
      availability: "available",
      command: "hermes",
      binaryPath: "hermes",
      defaultArgs: [],
      mcpCommand: null,
      modelEnvVar: null,
      providerEnvVar: null,
      thinkingEnvVar: null,
      maxTokensEnvVar: null,
      contextLimitEnvVar: null,
      maxRoundsEnvVar: null,
      installHint: "",
      installInstructionsUrl: "",
      canAutoInstall: false,
      underlyingCliPath: null,
      nodeRequired: false,
      authStatus: { status: "not_applicable" },
      loginHint: null,
      profileArg: "-p",
      providerLocked: true,
    },
    scope: "instance",
  });
  assert.equal(isModelOwnedByProfile(model), true);
});
