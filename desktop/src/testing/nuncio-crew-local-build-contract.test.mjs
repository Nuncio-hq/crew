import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const tauriRoot = resolve(repoRoot, "desktop/src-tauri");

test("local flavor is visibly Local while preserving the Buzz identity", () => {
  const config = JSON.parse(
    readFileSync(resolve(tauriRoot, "tauri.nuncio-crew.conf.json"), "utf8"),
  );
  const infoPlist = readFileSync(
    resolve(tauriRoot, "Info.NuncioCrew.plist"),
    "utf8",
  );
  const upstreamInfoPlist = readFileSync(
    resolve(tauriRoot, "Info.plist"),
    "utf8",
  );

  assert.equal(config.productName, "NuncioCrew Local");
  assert.equal(config.identifier, "xyz.block.buzz.app");
  assert.equal(config.bundle.macOS.infoPlist, "Info.NuncioCrew.plist");
  assert.equal(config.plugins, undefined);
  assert.match(
    infoPlist,
    /<key>CFBundleDisplayName<\/key>\s*<string>NuncioCrew Local<\/string>/,
  );
  assert.match(
    infoPlist,
    /<key>CFBundleName<\/key>\s*<string>NuncioCrew Local<\/string>/,
  );
  assert.equal(
    infoPlist
      .replaceAll("NuncioCrew Local", "Buzz")
      .replaceAll(/\s+/g, " ")
      .trim(),
    upstreamInfoPlist.replaceAll(/\s+/g, " ").trim(),
  );
});

test("local build marks its channel and deterministically disables updater secrets", () => {
  const buildScript = readFileSync(
    resolve(repoRoot, "scripts/build-nuncio-crew-local.sh"),
    "utf8",
  );
  const settingsView = readFileSync(
    resolve(repoRoot, "desktop/src/features/settings/ui/SettingsView.tsx"),
    "utf8",
  );

  for (const secretName of [
    "BUZZ_PRIVATE_KEY",
    "BUZZ_UPDATER_ENDPOINT",
    "BUZZ_UPDATER_PUBLIC_KEY",
    "TAURI_SIGNING_PRIVATE_KEY",
    "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    "OPENAI_COMPAT_API_KEY",
    "OPENROUTER_API_KEY",
    "DATABRICKS_TOKEN",
    "BUZZ_API_TOKEN",
    "BUZZ_ACP_API_TOKEN",
  ]) {
    assert.match(buildScript, new RegExp(`unset[\\s\\S]*${secretName}`));
  }
  assert.match(
    buildScript,
    /export VITE_NUNCIO_CREW_CHANNEL=(?:"|')?local(?:"|')?/,
  );
  assert.match(settingsView, /VITE_NUNCIO_CREW_CHANNEL/);
  assert.match(settingsView, />\s*Local\s*</);
});

test("local build packages real release sidecars without updater artifacts", () => {
  const buildScript = readFileSync(
    resolve(repoRoot, "scripts/build-nuncio-crew-local.sh"),
    "utf8",
  );

  assert.match(
    buildScript,
    /export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false/,
  );
  assert.match(buildScript, /cargo build --release/);
  assert.match(
    buildScript,
    /cargo build --release[\s\S]*--target "\$HOST_TARGET"/,
  );
  assert.match(buildScript, /scripts\/bundle-sidecars\.sh "\$HOST_TARGET"/);
  assert.match(buildScript, /tauri build/);
  assert.match(buildScript, /tauri build[\s\S]*--target "\$HOST_TARGET"/);
  assert.match(buildScript, /--bundles app/);
  assert.match(buildScript, /tauri\.nuncio-crew\.conf\.json/);
  assert.doesNotMatch(buildScript, /--debug/);
  assert.doesNotMatch(buildScript, /tauri\.release\.conf\.json/);
  assert.doesNotMatch(buildScript, /createUpdaterArtifacts/);
  assert.doesNotMatch(buildScript, /nsec/i);
});
