/**
 * Validation helpers for Hermes profile binding (Phase 02B + Phase 04 picker).
 */
import assert from "node:assert/strict";
import test from "node:test";

import {
  buildHermesProfileOccupancy,
  filterHermesProfileOptions,
  hermesProfileBindingError,
  hermesProfileOccupancyError,
  hermesProfileOccupancyLabel,
  normalizeHermesProfileList,
  profileOwnedModelLabel,
  runtimeOffersProfileBinding,
  runtimeOwnsModelViaProfile,
  shouldShowHermesProfileCreate,
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

test("normalizeHermesProfileList drops default/invalid and sorts", () => {
  assert.deepEqual(
    normalizeHermesProfileList([
      "builder",
      "default",
      "scout",
      "Bad",
      "scout",
      "",
    ]),
    ["builder", "scout"],
  );
});

test("filterHermesProfileOptions typeahead is case-insensitive", () => {
  assert.deepEqual(
    filterHermesProfileOptions(["scout", "builder", "ops"], "sc"),
    ["scout"],
  );
  assert.deepEqual(filterHermesProfileOptions(["scout", "builder"], ""), [
    "builder",
    "scout",
  ]);
});

test("shouldShowHermesProfileCreate only for valid missing names", () => {
  assert.equal(shouldShowHermesProfileCreate("research", ["scout"]), true);
  assert.equal(shouldShowHermesProfileCreate("scout", ["scout"]), false);
  assert.equal(shouldShowHermesProfileCreate("default", []), false);
  assert.equal(shouldShowHermesProfileCreate("Bad Name", []), false);
  assert.equal(shouldShowHermesProfileCreate("", ["scout"]), false);
});

test("buildHermesProfileOccupancy scopes by relay and edit-self", () => {
  const map = buildHermesProfileOccupancy({
    profiles: ["scout", "builder", "twin"],
    relayUrl: "wss://a",
    editingPubkey: "aaa",
    agents: [
      {
        pubkey: "aaa",
        name: "Self Scout",
        hermesProfile: "scout",
        relayUrl: "wss://a",
      },
      {
        pubkey: "bbb",
        name: "Other Builder",
        hermesProfile: "builder",
        relayUrl: "wss://a",
      },
      {
        pubkey: "ccc",
        name: "Other Relay",
        hermesProfile: "twin",
        relayUrl: "wss://b",
      },
    ],
  });
  assert.deepEqual(map.get("scout"), { status: "self" });
  assert.deepEqual(map.get("builder"), {
    status: "bound",
    agentName: "Other Builder",
    agentPubkey: "bbb",
  });
  assert.deepEqual(map.get("twin"), { status: "free" });
});

test("hermesProfileOccupancyError blocks bound-other only", () => {
  const occupancy = buildHermesProfileOccupancy({
    profiles: ["scout", "builder"],
    relayUrl: "wss://a",
    editingPubkey: "aaa",
    agents: [
      {
        pubkey: "aaa",
        name: "Me",
        hermesProfile: "scout",
        relayUrl: "wss://a",
      },
      {
        pubkey: "bbb",
        name: "Hermes Scout",
        hermesProfile: "builder",
        relayUrl: "wss://a",
      },
    ],
  });
  assert.equal(hermesProfileOccupancyError("scout", occupancy), null);
  assert.match(
    hermesProfileOccupancyError("builder", occupancy) ?? "",
    /already bound to agent 'Hermes Scout'/,
  );
  assert.equal(hermesProfileOccupancyError("research", occupancy), null);
  assert.equal(hermesProfileOccupancyLabel({ status: "free" }), "free");
  assert.equal(hermesProfileOccupancyLabel({ status: "self" }), "this agent");
  assert.equal(
    hermesProfileOccupancyLabel({
      status: "bound",
      agentName: "Hermes Scout",
      agentPubkey: "bbb",
    }),
    "bound · Hermes Scout",
  );
});

test("isNonOwnerOnlyRespondTo flags public respond-to modes", async () => {
  const { isNonOwnerOnlyRespondTo } = await import(
    "./HermesProfileCreateAffordance.tsx"
  );
  assert.equal(isNonOwnerOnlyRespondTo("anyone"), true);
  assert.equal(isNonOwnerOnlyRespondTo("allowlist"), true);
  assert.equal(isNonOwnerOnlyRespondTo("owner-only"), false);
  assert.equal(isNonOwnerOnlyRespondTo(null), false);
});

test("hermes profile command lines are auditable", async () => {
  const { hermesProfileCreateCommandLine, hermesProfileDeleteCommandLine } =
    await import("../../../shared/api/hermesProfiles.ts");
  assert.equal(
    hermesProfileCreateCommandLine("scout"),
    "hermes profile create scout --no-alias",
  );
  assert.equal(
    hermesProfileDeleteCommandLine("scout"),
    "hermes profile delete scout -y",
  );
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
