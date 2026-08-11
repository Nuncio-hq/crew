import assert from "node:assert/strict";
import test from "node:test";

import {
  deriveRuntimeCapabilities,
  runtimePersonaDocument,
} from "./runtimeCapabilities.ts";

function entry(metadata = {}) {
  return {
    id: "custom",
    label: "Custom",
    avatarUrl: "",
    availability: "available",
    command: "custom",
    binaryPath: "custom",
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
    requiresExternalCli: false,
    underlyingCliPath: null,
    nodeRequired: false,
    authStatus: { status: "not_applicable" },
    loginHint: null,
    source: "custom",
    ...metadata,
  };
}

test("capabilities derive Hermes profile ownership from catalog facts", () => {
  assert.deepEqual(
    deriveRuntimeCapabilities(
      entry({ profileArg: "-p", providerLocked: true, modelEnvVar: null }),
    ),
    {
      modelSource: "profileWriteThrough",
      personaDoc: "soulMd",
      layer3: "append",
    },
  );
});

test("non-Hermes and unknown runtimes use adapter settings without persona docs", () => {
  for (const runtime of [
    entry({ id: "claude" }),
    entry({ id: "codex" }),
    entry({ id: "goose", modelEnvVar: "GOOSE_MODEL" }),
    entry({ id: "buzz-agent", modelEnvVar: "BUZZ_AGENT_MODEL" }),
    entry({ id: "custom" }),
    undefined,
  ]) {
    assert.deepEqual(deriveRuntimeCapabilities(runtime), {
      modelSource: "adapterSetting",
      personaDoc: "none",
      layer3: "append",
    });
  }
});

test("persona filename is data, not a render-time runtime branch", () => {
  assert.equal(runtimePersonaDocument.soulMd, "SOUL.md");
  assert.equal(runtimePersonaDocument.none, null);
});
