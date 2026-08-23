import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { pathToFileURL, fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const workflowPath = resolve(
  repoRoot,
  ".github/workflows/nuncio-crew-release.yml",
);
const channelHelperPath = resolve(
  repoRoot,
  "desktop/scripts/nuncio-crew-release-channel.mjs",
);
const manifestHelperPath = resolve(
  repoRoot,
  "desktop/scripts/generate-nuncio-crew-latest-json.mjs",
);
const cargoLockHelperPath = resolve(
  repoRoot,
  "desktop/scripts/sync-nuncio-crew-cargo-lock-version.mjs",
);
const channelAdvanceHelperPath = resolve(
  repoRoot,
  "desktop/scripts/assert-nuncio-crew-channel-advance.mjs",
);
const updaterKeyHelperPath = resolve(
  repoRoot,
  "desktop/scripts/verify-nuncio-crew-updater-key-id.mjs",
);
const upstreamPinPath = resolve(repoRoot, "docs/crew/upstream-buzz.json");

const devEndpoint =
  "https://github.com/Nuncio-hq/crew/releases/download/nuncio-crew-dev-latest/latest.json";
const stableEndpoint =
  "https://github.com/Nuncio-hq/crew/releases/download/nuncio-crew-stable-latest/latest.json";

async function loadChannelHelper() {
  return import(pathToFileURL(channelHelperPath).href);
}

test("release workflow is manual-only and scoped to Nuncio Crew", () => {
  const workflow = readFileSync(workflowPath, "utf8");
  const triggerBlock = workflow.slice(
    workflow.indexOf("on:"),
    workflow.indexOf("\njobs:"),
  );

  assert.match(triggerBlock, /^on:\s*\n\s+workflow_dispatch:/);
  assert.doesNotMatch(triggerBlock, /^\s+(push|pull_request|schedule):/m);
  assert.match(workflow, /github\.repository\s*==\s*['"]Nuncio-hq\/crew['"]/);
  assert.match(workflow, /github\.ref\s*==\s*['"]refs\/heads\/main['"]/);
  assert.match(workflow, /^\s+environment:\s*nuncio-crew-release$/m);
  assert.doesNotMatch(workflow, /block\/apple-codesign-action/i);
  assert.doesNotMatch(workflow, /github\.com\/block\/buzz/i);
});

test("manual release inputs fail closed and target macOS arm64", () => {
  const workflow = readFileSync(workflowPath, "utf8");

  for (const input of ["version", "channel", "ref"]) {
    assert.match(
      workflow,
      new RegExp(`${input}:\\s*\\n(?:\\s+.*\\n)*?\\s+required:\\s*true`),
    );
  }
  assert.match(workflow, /options:\s*\n\s+- dev\s*\n\s+- stable/);
  assert.match(workflow, /git rev-parse --verify/);
  assert.match(workflow, /\^\{commit\}/);
  assert.match(workflow, /aarch64-apple-darwin/);
  assert.match(workflow, /runs-on:\s*macos-/);
  assert.match(workflow, /publish:[\s\S]*default:\s*false/);
  assert.match(workflow, /if:\s*inputs\.publish/);
  assert.match(workflow, /tauri\.nuncio-crew-release\.conf\.json/);
  assert.match(workflow, /tauri\.release\.conf\.json/);
  assert.match(workflow, /latest\.json/);
  assert.match(workflow, /NUNCIO_TAURI_SIGNING_PRIVATE_KEY/);
  assert.match(workflow, /NUNCIO_APPLE_API_PRIVATE_KEY/);
  assert.match(workflow, /verify-nuncio-crew-updater-key-id\.mjs/);
  assert.match(workflow, /verify-macos-entitlements\.sh/);
  assert.match(
    workflow,
    /MAIN_SHA=.*refs\/remotes\/origin\/main[\s\S]*HEAD_SHA.*MAIN_SHA/,
  );
  assert.match(
    workflow,
    /security delete-keychain[\s\\]*"\$RUNNER_TEMP\/nuncio-crew/,
  );
});

test("release publishes immutable assets before advancing updater channels", () => {
  const workflow = readFileSync(workflowPath, "utf8");
  const publishVersion = workflow.indexOf(
    'gh release edit "$RELEASE_TAG" --draft=false',
  );
  const updateRollingManifest = workflow.indexOf(
    'gh release upload "$rolling" latest.json --clobber',
  );

  assert.notEqual(publishVersion, -1);
  assert.notEqual(updateRollingManifest, -1);
  assert.ok(publishVersion < updateRollingManifest);
});

test("release collision checks use the Crew-owned immutable tag", () => {
  const workflow = readFileSync(workflowPath, "utf8");
  const validation = workflow.indexOf("- name: Validate version and channel");
  const collisionCheck = workflow.indexOf(
    "- name: Verify Crew release tag is available",
  );
  const collisionStep = workflow.slice(
    collisionCheck,
    workflow.indexOf("\n      - ", collisionCheck + 1),
  );

  assert.notEqual(validation, -1);
  assert.notEqual(collisionCheck, -1);
  assert.ok(validation < collisionCheck);
  assert.doesNotMatch(
    workflow,
    /RELEASE_TAG:\s*\$\{\{\s*inputs\.version\s*\}\}/,
  );
  assert.match(
    collisionStep,
    /RELEASE_TAG:\s*\$\{\{\s*steps\.release\.outputs\.release_tag\s*\}\}/,
  );
  assert.match(collisionStep, /"refs\/tags\/\$RELEASE_TAG"/);
});

test("release notarizes and staples the DMG before validating its ticket", () => {
  const workflow = readFileSync(workflowPath, "utf8");
  const notarizeStart = workflow.indexOf("- name: Notarize and staple DMG");
  const verifyStart = workflow.indexOf("- name: Verify and locate artifacts");
  const notarizeStep = workflow.slice(notarizeStart, verifyStart);
  const submitDmg = notarizeStep.indexOf('xcrun notarytool submit "$DMG"');
  const stapleDmg = notarizeStep.indexOf('xcrun stapler staple "$DMG"');
  const validateDmg = workflow.indexOf('xcrun stapler validate "$DMG"');

  assert.notEqual(notarizeStart, -1);
  assert.notEqual(verifyStart, -1);
  assert.match(notarizeStep, /set -euo pipefail/);
  assert.notEqual(submitDmg, -1);
  assert.notEqual(stapleDmg, -1);
  assert.notEqual(validateDmg, -1);
  for (const flag of ["--key", "--key-id", "--issuer", "--wait"]) {
    assert.match(notarizeStep, new RegExp(`\\s${flag}(?:\\s|$)`));
  }
  assert.ok(submitDmg < stapleDmg);
  assert.ok(workflow.indexOf('xcrun stapler staple "$DMG"') < validateDmg);
});

test("release changes only the root package version in Cargo.lock", async () => {
  const workflow = readFileSync(workflowPath, "utf8");

  assert.doesNotMatch(workflow, /cargo update(?:\s|\\)+--workspace/);
  assert.match(
    workflow,
    /node scripts\/sync-nuncio-crew-cargo-lock-version\.mjs/,
  );

  const { replaceCargoLockPackageVersion } = await import(
    pathToFileURL(cargoLockHelperPath).href
  );
  const original = `# generated

[[package]]
name = "buzz-desktop"
version = "0.5.3"
dependencies = [
 "tauri",
]

[[package]]
name = "tauri"
version = "2.8.5"
`;

  assert.equal(
    replaceCargoLockPackageVersion(original, "buzz-desktop", "0.0.1-dev"),
    original.replace(
      'name = "buzz-desktop"\nversion = "0.5.3"',
      'name = "buzz-desktop"\nversion = "0.0.1-dev"',
    ),
  );
});

test("all releases serialize and updater channels only advance", async () => {
  const workflow = readFileSync(workflowPath, "utf8");

  assert.match(workflow, /group:\s*nuncio-crew-release\s*$/m);
  assert.doesNotMatch(workflow, /group:.*inputs\.version/);
  assert.match(workflow, /assert-nuncio-crew-channel-advance\.mjs/);

  const { compareNuncioCrewVersions } = await import(
    pathToFileURL(channelAdvanceHelperPath).href
  );
  assert.ok(compareNuncioCrewVersions("0.0.1-dev", "0.0.1-dev.1") < 0);
  assert.ok(compareNuncioCrewVersions("0.0.1-dev.7", "0.0.1") < 0);
  assert.ok(compareNuncioCrewVersions("0.0.2-dev", "0.0.1") > 0);
  assert.equal(compareNuncioCrewVersions("1.2.3", "1.2.3"), 0);
});

test("updater signature key must match the embedded public key", async () => {
  const { updaterKeyId, verifyUpdaterKeyId } = await import(
    pathToFileURL(updaterKeyHelperPath).href
  );
  const publicPayload = Buffer.concat([
    Buffer.from("Ed"),
    Buffer.from("0123456789abcdef", "hex"),
    Buffer.alloc(32, 1),
  ]).toString("base64");
  const signaturePayload = Buffer.concat([
    Buffer.from("ED"),
    Buffer.from("0123456789abcdef", "hex"),
    Buffer.alloc(64, 2),
  ]).toString("base64");
  const publicKey = Buffer.from(
    `untrusted comment: minisign public key\n${publicPayload}\n`,
  ).toString("base64");
  const signature = Buffer.from(
    `untrusted comment: signature\n${signaturePayload}\n`,
  ).toString("base64");
  const mismatchedSignaturePayload = Buffer.concat([
    Buffer.from("ED"),
    Buffer.from("fedcba9876543210", "hex"),
    Buffer.alloc(64, 2),
  ]).toString("base64");
  const mismatchedSignature = Buffer.from(
    `untrusted comment: signature\n${mismatchedSignaturePayload}\n`,
  ).toString("base64");

  assert.equal(updaterKeyId(publicKey), "0123456789abcdef");
  assert.doesNotThrow(() => verifyUpdaterKeyId(publicKey, signature));
  assert.throws(() => verifyUpdaterKeyId(publicKey, mismatchedSignature));
});

test("release helper classifies dev and stable versions with isolated endpoints", async () => {
  const { classifyNuncioCrewRelease } = await loadChannelHelper();

  assert.deepEqual(classifyNuncioCrewRelease("v0.0.1-dev"), {
    version: "0.0.1-dev",
    releaseTag: "crew-v0.0.1-dev",
    channel: "dev",
    prerelease: true,
    rollingTags: ["nuncio-crew-dev-latest"],
    updaterEndpoint: devEndpoint,
  });
  assert.deepEqual(classifyNuncioCrewRelease("v1.2.3"), {
    version: "1.2.3",
    releaseTag: "crew-v1.2.3",
    channel: "stable",
    prerelease: false,
    rollingTags: ["nuncio-crew-stable-latest", "nuncio-crew-dev-latest"],
    updaterEndpoint: stableEndpoint,
  });
  assert.notEqual(devEndpoint, stableEndpoint);
  assert.equal(
    classifyNuncioCrewRelease("v1.2.4-dev.7").version,
    "1.2.4-dev.7",
  );
  assert.equal(
    classifyNuncioCrewRelease("v1.2.4-dev.7").releaseTag,
    "crew-v1.2.4-dev.7",
  );
  assert.notEqual(classifyNuncioCrewRelease("v0.0.5").releaseTag, "v0.0.5");
});

test("release helper rejects unsupported versions and mismatched channels", async () => {
  const { validateNuncioCrewReleaseRequest } = await loadChannelHelper();

  assert.throws(() =>
    validateNuncioCrewReleaseRequest({
      version: "0.0.1-dev",
      channel: "dev",
      ref: "main",
    }),
  );
  assert.throws(() =>
    validateNuncioCrewReleaseRequest({
      version: "v0.0.1-beta",
      channel: "dev",
      ref: "main",
    }),
  );
  assert.throws(() =>
    validateNuncioCrewReleaseRequest({
      version: "v0.0.1-dev",
      channel: "stable",
      ref: "main",
    }),
  );
  assert.throws(() =>
    validateNuncioCrewReleaseRequest({
      version: "v0.0.1-dev",
      channel: "dev",
      ref: "",
    }),
  );
  assert.throws(() =>
    validateNuncioCrewReleaseRequest({
      version: "v0.0.1-dev.alpha",
      channel: "dev",
      ref: "a".repeat(40),
    }),
  );
});

test("release identity and updater manifest are Nuncio-owned", async () => {
  const releaseConfig = JSON.parse(
    readFileSync(
      resolve(
        repoRoot,
        "desktop/src-tauri/tauri.nuncio-crew-release.conf.json",
      ),
      "utf8",
    ),
  );
  assert.equal(releaseConfig.productName, "NuncioCrew");
  assert.equal(releaseConfig.identifier, "com.nuncio.crew");

  const { buildNuncioCrewUpdateManifest } = await import(
    pathToFileURL(manifestHelperPath).href
  );
  const manifest = buildNuncioCrewUpdateManifest({
    versionTag: "v0.0.5",
    signature: "signed-update",
    archiveUrl:
      "https://github.com/Nuncio-hq/crew/releases/download/crew-v0.0.5/NuncioCrew.app.tar.gz",
    publishedAt: new Date("2026-07-30T00:00:00.000Z"),
  });

  assert.deepEqual(manifest, {
    version: "0.0.5",
    notes: "NuncioCrew v0.0.5",
    pub_date: "2026-07-30T00:00:00.000Z",
    platforms: {
      "darwin-aarch64": {
        signature: "signed-update",
        url: "https://github.com/Nuncio-hq/crew/releases/download/crew-v0.0.5/NuncioCrew.app.tar.gz",
      },
    },
  });
  assert.throws(() =>
    buildNuncioCrewUpdateManifest({
      versionTag: "v0.0.1-dev",
      signature: " ",
      archiveUrl: "https://example.invalid/update.tar.gz",
    }),
  );
  assert.throws(() =>
    buildNuncioCrewUpdateManifest({
      versionTag: "v0.0.5",
      signature: "signed-update",
      archiveUrl:
        "https://github.com/Nuncio-hq/crew/releases/download/v0.0.5/NuncioCrew.app.tar.gz",
    }),
  );
});

const EXPECTED_UPSTREAM_PIN = {
  buzzVersion: "0.5.18",
  buzzTag: "desktop-v0.5.18",
  buzzCommit: "39f8b46935736334cdd7045a4e4b5d7eb1a33888",
};

test("Buzz manifests stay pinned and the exact upstream source is machine-readable", () => {
  const packageJson = JSON.parse(
    readFileSync(resolve(repoRoot, "desktop/package.json"), "utf8"),
  );
  const tauriConfig = JSON.parse(
    readFileSync(
      resolve(repoRoot, "desktop/src-tauri/tauri.conf.json"),
      "utf8",
    ),
  );
  const cargoToml = readFileSync(
    resolve(repoRoot, "desktop/src-tauri/Cargo.toml"),
    "utf8",
  );
  const cargoLock = readFileSync(
    resolve(repoRoot, "desktop/src-tauri/Cargo.lock"),
    "utf8",
  );
  const settingsView = readFileSync(
    resolve(repoRoot, "desktop/src/features/settings/ui/SettingsView.tsx"),
    "utf8",
  );
  const upstreamPin = JSON.parse(readFileSync(upstreamPinPath, "utf8"));

  assert.deepEqual(upstreamPin, EXPECTED_UPSTREAM_PIN);
  assert.equal(packageJson.version, EXPECTED_UPSTREAM_PIN.buzzVersion);
  assert.equal(tauriConfig.version, EXPECTED_UPSTREAM_PIN.buzzVersion);
  assert.match(cargoToml, /^\[package\][\s\S]*?^version = "0\.5\.18"$/m);
  assert.match(
    cargoLock,
    /^\[\[package\]\]\nname = "buzz-desktop"\nversion = "0\.5\.18"$/m,
  );
  assert.match(settingsView, /from "@tauri-apps\/api\/app"/);
  assert.match(settingsView, /void getVersion\(\)\.then\(setAppVersion\)/);
  assert.match(settingsView, /data-testid="settings-version"/);
  assert.match(settingsView, /v\{appVersion\}/);
});
