/**
 * Validation helpers for Hermes profile binding (Phase 02B + Phase 04 picker).
 */
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";

const [bindingFieldsSource, createBindingSource, editBindingSource] =
  await Promise.all([
    readFile(
      new URL("./HermesProfileBindingFields.tsx", import.meta.url),
      "utf8",
    ),
    readFile(
      new URL("./createHermesBindingFields.tsx", import.meta.url),
      "utf8",
    ),
    readFile(new URL("./editHermesBinding.ts", import.meta.url), "utf8"),
  ]);

import * as hermesProfileBinding from "../lib/hermesProfileBinding.ts";
import {
  buildHermesProfileOccupancy,
  filterHermesProfileOptions,
  hermesProfileBindingError,
  hermesProfileOccupancyError,
  hermesProfileOccupancyLabel,
  normalizeHermesProfileList,
  profileOwnedModelLabel,
  resolveHermesProfileForCreate,
  resolveHermesProfileForUpdate,
  runtimeOffersProfileBinding,
  runtimeOwnsModelViaProfile,
  shouldClearHermesProfileOnRuntimeChange,
  shouldShowHermesProfileCreate,
  validateHermesProfileName,
} from "../lib/hermesProfileBinding.ts";
import {
  isModelOwnedByProfile,
  isModelWriteThrough,
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

test("runtime switches clear hidden Hermes profile state", () => {
  const profileRuntime = { profileArg: "-p" };
  const plainRuntime = { profileArg: null };

  assert.equal(resolveHermesProfileForCreate("scout", profileRuntime), "scout");
  assert.equal(resolveHermesProfileForCreate("scout", plainRuntime), null);
  assert.equal(
    resolveHermesProfileForUpdate("scout", "scout", profileRuntime),
    undefined,
  );
  assert.equal(
    resolveHermesProfileForUpdate("scout", "builder", profileRuntime),
    "builder",
  );
  assert.equal(
    resolveHermesProfileForUpdate("scout", "scout", plainRuntime),
    null,
  );
  assert.equal(
    resolveHermesProfileForUpdate("scout", "scout", undefined),
    undefined,
  );
  assert.equal(
    resolveHermesProfileForUpdate(null, "", plainRuntime),
    undefined,
  );
  assert.equal(shouldClearHermesProfileOnRuntimeChange(profileRuntime), false);
  assert.equal(shouldClearHermesProfileOnRuntimeChange(plainRuntime), true);
  assert.equal(shouldClearHermesProfileOnRuntimeChange(undefined), false);
  assert.equal(shouldClearHermesProfileOnRuntimeChange(undefined, true), true);
});

test("profile binding projects the trusted owner-local boundary", () => {
  assert.deepEqual(
    hermesProfileBinding.deriveProfileBoundAgentBoundary({
      profileBindingOffered: true,
      profile: "scout",
      usedIn: ["Product", "Research"],
    }),
    {
      access: "Owner only",
      autonomy: "Full",
      backend: "This Mac",
      profile: "scout",
      usedIn: ["Product", "Research"],
    },
  );
  assert.equal(
    hermesProfileBinding.deriveProfileBoundAgentBoundary({
      profileBindingOffered: false,
      profile: "",
      usedIn: [],
    }),
    null,
  );
});

test("trusted boundary card shows profile reuse as shared-state information", async () => {
  const { ProfileBoundAgentBoundaryCard } = await import(
    "./HermesProfileBindingFields.tsx"
  );
  const html = renderToStaticMarkup(
    createElement(ProfileBoundAgentBoundaryCard, {
      boundary: {
        access: "Owner only",
        autonomy: "Full",
        backend: "This Mac",
        profile: "scout",
        usedIn: ["Product", "Research"],
      },
      hasPresentationMismatch: false,
      otherUses: [],
    }),
  );

  for (const text of [
    "Access",
    "Owner only",
    "Autonomy",
    "Full",
    "Backend",
    "This Mac",
    "Profile",
    "scout",
    "Used in",
    "Product, Research",
    "Crew approves ACP tool requests automatically;",
    "own approval policy still applies.",
    "One managed agent uses this profile across its configured communities.",
    "Memory, skills, and profile state are shared.",
  ]) {
    assert.ok(html.includes(text), `missing visible boundary copy: ${text}`);
  }
});

test("profile field derives and renders the boundary from capability data", () => {
  assert.match(bindingFieldsSource, /deriveHermesProfileUsage\(\{/);
  assert.match(bindingFieldsSource, /deriveProfileBoundAgentBoundary\(\{/);
  assert.match(bindingFieldsSource, /<ProfileBoundAgentBoundaryCard/);
  assert.match(bindingFieldsSource, /currentAgentName/);
  assert.doesNotMatch(bindingFieldsSource, /runtime\.id/);
});

test("profile-bound access and backend violations have actionable copy", () => {
  assert.equal(
    hermesProfileBinding.profileBoundAccessError(true, "anyone"),
    "Hermes profile agents use full autonomy and must stay owner-only. Choose Only me to continue.",
  );
  assert.equal(
    hermesProfileBinding.profileBoundAccessError(true, "owner-only"),
    null,
  );
  assert.equal(
    hermesProfileBinding.profileBoundBackendError(true, "remote"),
    "Hermes profiles live on this Mac and cannot run on a remote backend. Choose This computer to continue.",
  );
  assert.equal(
    hermesProfileBinding.profileBoundBackendError(true, "local"),
    null,
  );
  assert.equal(
    hermesProfileBinding.profileBoundBackendError(true, "remote", true),
    "Hermes profiles live on this Mac and cannot run on a remote backend. Delete and recreate this agent on This computer to continue.",
  );
  assert.equal(
    hermesProfileBinding.profileBoundAccessError(false, "anyone"),
    null,
  );
});

test("create and edit save gates consume the trusted-boundary error", () => {
  assert.match(createBindingSource, /blockingError/);
  assert.match(createBindingSource, /profileError: blockingError/);
  assert.match(editBindingSource, /blockingError/);
  assert.match(editBindingSource, /hermesProfileError: blockingError/);
  assert.match(bindingFieldsSource, /hermes-trusted-boundary-error/);
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

test("buildHermesProfileOccupancy is global and preserves edit-self", () => {
  const map = buildHermesProfileOccupancy({
    profiles: ["scout", "builder", "twin"],
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
        name: "Other Agent",
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
  assert.deepEqual(map.get("twin"), {
    status: "bound",
    agentName: "Other Agent",
    agentPubkey: "ccc",
  });
});

test("profile usage follows configured communities, not obsolete record pins", () => {
  const usage = hermesProfileBinding.deriveHermesProfileUsage({
    profile: "scout",
    currentAgentName: "Scout",
    currentRelayUrl: "wss://product.example",
    editingPubkey: "aaa",
    communities: [
      {
        name: "Product",
        relayUrl: "wss://product.example",
      },
      {
        name: "Research",
        relayUrl: "wss://research.example",
      },
    ],
    agents: [
      {
        pubkey: "aaa",
        name: "Scout",
        hermesProfile: "scout",
        relayUrl: "wss://product.example",
      },
      {
        pubkey: "bbb",
        name: "Scout",
        hermesProfile: "scout",
        relayUrl: "wss://research.example",
      },
      {
        pubkey: "ccc",
        name: "Builder",
        hermesProfile: "builder",
        relayUrl: "wss://ops.example",
      },
    ],
  });

  assert.deepEqual(usage, {
    usedIn: ["Product", "Research"],
    otherUses: [],
    hasPresentationMismatch: false,
  });
});

test("profile usage survives native records without a relay identity", () => {
  const usage = hermesProfileBinding.deriveHermesProfileUsage({
    profile: "scout",
    currentRelayUrl: "wss://product.example",
    currentAgentName: "Scout",
    communities: [
      { name: "Product", relayUrl: "wss://product.example" },
      { name: "Research", relayUrl: "wss://research.example" },
    ],
    agents: [
      {
        pubkey: "a".repeat(64),
        name: "Scout",
        relayUrl: "",
        hermesProfile: "scout",
      },
    ],
  });
  assert.deepEqual(usage, {
    usedIn: ["Product", "Research"],
    otherUses: [],
    hasPresentationMismatch: false,
  });
});

test("hermesProfileOccupancyError blocks bound-other only", () => {
  const occupancy = buildHermesProfileOccupancy({
    profiles: ["scout", "builder"],
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

test("profile write-through no longer uses the owned-model omission", () => {
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
  assert.equal(isModelOwnedByProfile(model), false);
  assert.equal(isModelWriteThrough(model), true);
});
