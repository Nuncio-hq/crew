import assert from "node:assert/strict";
import test from "node:test";

import { defaultRecapSettings, hasValidRecapSelection } from "./types.ts";
import { recapActionLabel, recapStatusLabel } from "./ui/ThreadRecapPanel.tsx";
import {
  canSaveRecapSettings,
  recapSettingsLoadState,
} from "./ui/RecapSettingsCard.tsx";

const hermes = {
  id: "hermes",
  label: "Hermes",
  kind: "hermes",
  availability: "supported",
  reason: null,
  capabilityFingerprint: "fp-1",
  profiles: [{ id: "research", label: "Research" }],
  // The backend only reports a supported runtime after admission, and always
  // carries that admitted model in `models`; manual mode must request it.
  models: ["hermes-certified-model"],
};

test("recap defaults are Off and cannot silently select a runtime", () => {
  const settings = defaultRecapSettings();
  assert.equal(settings.mode, "off");
  assert.equal(settings.runtimeId, null);
  assert.equal(hasValidRecapSelection(settings, [hermes]), true);
  assert.equal(
    hasValidRecapSelection(
      { ...settings, mode: "manual", runtimeId: "missing" },
      [hermes],
    ),
    false,
  );
});

test("Hermes recap settings require a profile and its certified model", () => {
  const snapshot = { settings: defaultRecapSettings(), runtimes: [hermes] };
  const manual = {
    ...snapshot.settings,
    mode: "manual",
    runtimeId: "hermes",
    capabilityFingerprint: "fp-1",
    // Backend validate_settings requires requestedModel == the admitted model
    // for every kind; the UI fills it from runtime.models when a profile or a
    // single-model runtime is chosen.
    requestedModel: "hermes-certified-model",
  };
  assert.equal(canSaveRecapSettings(manual, snapshot), false);
  assert.equal(
    canSaveRecapSettings({ ...manual, requestedModel: null }, snapshot),
    false,
  );
  assert.equal(
    canSaveRecapSettings(
      { ...manual, requestedModel: "invented-model" },
      snapshot,
    ),
    false,
  );
  assert.equal(
    canSaveRecapSettings({ ...manual, profileRef: "research" }, snapshot),
    true,
  );
  // Backend validate_settings demands requestedModel for hermes too: a valid
  // profile alone is not enough.
  assert.equal(
    hasValidRecapSelection(
      { ...manual, profileRef: "research", requestedModel: null },
      [hermes],
    ),
    false,
  );
  assert.equal(
    canSaveRecapSettings(
      { ...manual, profileRef: "research", capabilityFingerprint: "old" },
      snapshot,
    ),
    false,
  );
  assert.equal(
    canSaveRecapSettings(
      { ...manual, profileRef: "research", capabilityFingerprint: null },
      snapshot,
    ),
    false,
  );
  assert.equal(
    recapSettingsLoadState(null, "command unavailable"),
    "unavailable",
  );
});

test("CLI recap settings must choose a backend-discovered model", () => {
  const runtime = {
    ...hermes,
    id: "claude",
    kind: "cli",
    profiles: [],
    models: ["certified-model"],
    capabilityFingerprint: "fp-cli",
  };
  const snapshot = { settings: defaultRecapSettings(), runtimes: [runtime] };
  const base = {
    ...snapshot.settings,
    mode: "manual",
    runtimeId: "claude",
    capabilityFingerprint: "fp-cli",
  };
  assert.equal(canSaveRecapSettings(base, snapshot), false);
  assert.equal(
    canSaveRecapSettings(
      { ...base, requestedModel: "invented-model" },
      snapshot,
    ),
    false,
  );
  assert.equal(
    canSaveRecapSettings(
      { ...base, requestedModel: " certified-model " },
      snapshot,
    ),
    true,
  );
});

test("recap status and action labels keep cancellation and stale output explicit", () => {
  assert.equal(recapStatusLabel("unsupported"), "Unavailable");
  assert.equal(recapStatusLabel("stale"), "Out of date");
  assert.equal(
    recapActionLabel({ status: "generating", recap: null }),
    "Generating…",
  );
  assert.equal(
    recapActionLabel({ status: "current", recap: { text: "old" } }),
    "Regenerate recap",
  );
});

test("the first render keeps both settings and Context honest while native support is loading", async () => {
  const React = await import("react");
  const { renderToStaticMarkup } = await import("react-dom/server");
  const { RecapSettingsCard } = await import("./ui/RecapSettingsCard.tsx");
  const { ThreadRecapPanel } = await import("./ui/ThreadRecapPanel.tsx");
  const settingsHtml = renderToStaticMarkup(
    React.createElement(RecapSettingsCard, {
      client: {
        getSettings: () => new Promise(() => {}),
        saveSettings: async () => {
          throw new Error("unused");
        },
        getThreadRecap: async () => {
          throw new Error("unused");
        },
        generateThreadRecap: async () => {
          throw new Error("unused");
        },
        cancelThreadRecap: async () => {},
      },
    }),
  );
  const threadHtml = renderToStaticMarkup(
    React.createElement(ThreadRecapPanel, {
      channelId: "channel-1",
      rootEventId: "root-1",
      client: {
        getSettings: () => new Promise(() => {}),
        saveSettings: async () => {
          throw new Error("unused");
        },
        getThreadRecap: async () => {
          throw new Error("unused");
        },
        generateThreadRecap: async () => {
          throw new Error("unused");
        },
        cancelThreadRecap: async () => {},
      },
    }),
  );
  assert.match(settingsHtml, /Checking recap runtime support/);
  assert.match(threadHtml, /Checking runtime support/);
  assert.match(threadHtml, /thread-recap-generate/);
});
