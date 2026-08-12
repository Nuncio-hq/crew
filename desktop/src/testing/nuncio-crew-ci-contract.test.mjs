import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { pathToFileURL, fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const gateHelperPath = resolve(
  repoRoot,
  "desktop/scripts/check-nuncio-crew-ci-results.mjs",
);

function workflow(name) {
  return readFileSync(resolve(repoRoot, `.github/workflows/${name}`), "utf8");
}

test("Crew CI exposes one stable merge gate", () => {
  const ci = workflow("nuncio-crew-ci.yml");

  assert.match(ci, /^name:\s*NuncioCrew CI$/m);
  assert.match(ci, /^\s+pull_request:\s*$/m);
  assert.match(ci, /^\s+push:\s*$/m);
  assert.match(ci, /^\s+branches:\s*\[main\]\s*$/m);
  assert.match(ci, /^\s+name:\s*NuncioCrew Gate$/m);
  assert.match(ci, /if:\s*\$\{\{\s*always\(\)\s*\}\}/);
  assert.match(ci, /check-nuncio-crew-ci-results\.mjs/);
  for (const job of [
    "changes",
    "desktop-fast",
    "desktop-rust",
    "macos-arm",
    "project-relay",
    "buzz-acp",
  ]) {
    assert.match(ci, new RegExp(`needs\\.${job}\\.result`));
  }
  assert.doesNotMatch(ci, /pull_request_target|schedule:/);
});

test("automatic Crew CI is macOS ARM and desktop only", () => {
  const ci = workflow("nuncio-crew-ci.yml");

  assert.match(ci, /aarch64-apple-darwin/);
  assert.match(ci, /tauri\.nuncio-crew-release\.conf\.json/);
  assert.match(ci, /--no-sign/);
  assert.match(ci, /- 'Justfile'/);
  assert.match(ci, /fetch-depth:\s*2/);
  assert.doesNotMatch(ci, /mesh-llm|LLAMA_STAGE|SKIPPY_LLAMA/);
  assert.doesNotMatch(
    ci,
    /flutter|windows-latest|linux\/amd64|linux\/arm64|helm|Dockerfile/,
  );
  assert.doesNotMatch(
    ci,
    /APPLE_CERTIFICATE|APPLE_API|TAURI_SIGNING_PRIVATE_KEY|contents:\s*write/,
  );
});

test("relay-native Project behavior remains an automatic conditional gate", () => {
  const ci = workflow("nuncio-crew-ci.yml");

  assert.match(ci, /name:\s*Project Relay/);
  assert.match(ci, /project-local-workspace-live-relay\.test\.mjs/);
  assert.match(ci, /CREW_LIVE_RELAY_URL/);
  assert.match(ci, /desktop\/src\/features\/projects\/\*\*/);
  assert.match(ci, /- 'crates\/\*\*'/);
  assert.match(ci, /- 'docker-compose\.yml'/);
  assert.match(ci, /- 'scripts\/attach-schema-partitions\.sql'/);
  assert.match(ci, /CREW_LIVE_RELAY_URL:\s*ws:\/\/localhost:3000/);
  assert.match(ci, /needs\.project-relay\.result/);
});

test("buzz-acp is path-gated and registered in the merge gate", () => {
  const ci = workflow("nuncio-crew-ci.yml");
  const acpStart = ci.indexOf("\n  buzz-acp:");
  assert.ok(acpStart > 0, "buzz-acp job must exist");
  const nextJob = ci.indexOf("\n  desktop-smoke-e2e:", acpStart);
  const acp = ci.slice(acpStart, nextJob > acpStart ? nextJob : undefined);

  assert.match(acp, /name:\s*buzz-acp/);
  assert.match(acp, /needs\.changes\.outputs\.acp == 'true'/);
  assert.match(ci, /acp:\s*\n(?:\s+- '[^']+'\n)*\s+- 'crates\/buzz-acp\/\*\*'/);
  // Path dep of buzz-acp — source edits here do not touch Cargo.lock.
  assert.match(
    ci,
    /acp:\s*\n(?:\s+- '[^']+'\n)*\s+- 'crates\/buzz-persona\/\*\*'/,
  );
  assert.match(acp, /just buzz-acp-test/);
  assert.match(ci, /needs\.buzz-acp\.result/);

  const gateHelper = readFileSync(gateHelperPath, "utf8");
  assert.match(gateHelper, /"buzz-acp":\s*"acp"/);
});

test("desktop rust is path-gated and registered in the merge gate", () => {
  const ci = workflow("nuncio-crew-ci.yml");
  const rustStart = ci.indexOf("\n  desktop-rust:");
  assert.ok(rustStart > 0, "desktop-rust job must exist");
  const nextJob = ci.indexOf("\n  macos-arm:", rustStart);
  const rust = ci.slice(rustStart, nextJob > rustStart ? nextJob : undefined);

  assert.match(rust, /name:\s*Desktop Rust/);
  assert.match(rust, /needs\.changes\.outputs\.desktop-rust == 'true'/);
  // Filter must cover Tauri path deps in crates/ — otherwise a crates-only PR
  // skips Desktop Rust while macOS ARM only catches compile, not clippy/tests.
  assert.match(
    ci,
    /desktop-rust:\s*\n(?:\s+- '[^']+'\n)*\s+- 'desktop\/src-tauri\/\*\*'\n(?:\s+- '[^']+'\n)*\s+- 'crates\/\*\*'/,
  );
  assert.match(
    ci,
    /desktop-rust:\s*\n(?:\s+- '[^']+'\n)*\s+- 'Cargo\.toml'\n\s+- 'Cargo\.lock'/,
  );
  assert.match(rust, /workspaces:\s*desktop\/src-tauri/);
  assert.match(rust, /libwebkit2gtk-4\.1-dev/);
  assert.match(rust, /DPkg::Lock::Timeout=120/);
  assert.match(rust, /just desktop-tauri-clippy/);
  assert.match(rust, /just desktop-tauri-test/);
  assert.match(rust, /CMAKE_POLICY_VERSION_MINIMUM:\s*"3\.5"/);
  // Cost controls: leave check + compiled-flags to Upstream Sync.
  assert.doesNotMatch(rust, /desktop-tauri-check/);
  assert.doesNotMatch(rust, /desktop-tauri-test-compiled-flags/);
  assert.match(ci, /desktop-rust-changed/);
  assert.match(ci, /needs\.desktop-rust\.result/);

  const gateHelper = readFileSync(gateHelperPath, "utf8");
  assert.match(gateHelper, /"desktop-rust":\s*"desktop-rust-changed"/);
});

test("desktop smoke e2e runs on PRs as an advisory signal until flakes are triaged", () => {
  const ci = workflow("nuncio-crew-ci.yml");
  const smokeStart = ci.indexOf("desktop-smoke-e2e:");
  assert.ok(smokeStart > 0, "desktop-smoke-e2e job must exist");
  const nextJob = ci.indexOf("\n  desktop-e2e-integration:", smokeStart);
  const gateStart = ci.indexOf("\n  gate:", smokeStart);
  const smokeEnd =
    nextJob > smokeStart
      ? nextJob
      : gateStart > smokeStart
        ? gateStart
        : undefined;
  const smoke = ci.slice(smokeStart, smokeEnd);

  assert.match(smoke, /name:\s*Desktop Smoke E2E/);
  assert.match(smoke, /continue-on-error:\s*true/);
  assert.match(smoke, /shard:\s*\[1,\s*2,\s*3,\s*4\]/);
  assert.match(smoke, /pnpm -C desktop build:e2e/);
  assert.match(
    smoke,
    /playwright test --project=smoke --shard=\$\{\{ matrix\.shard \}\}\/4/,
  );
  assert.match(smoke, /needs\.changes\.outputs\.desktop == 'true'/);
  // Advisory: must not be registered in the merge gate while flakes remain.
  assert.doesNotMatch(ci, /needs\.desktop-smoke-e2e\.result/);
  const gateHelper = readFileSync(
    resolve(repoRoot, "desktop/scripts/check-nuncio-crew-ci-results.mjs"),
    "utf8",
  );
  assert.doesNotMatch(gateHelper, /desktop-smoke-e2e/);
});

test("desktop e2e integration runs on PRs as an advisory relay-backed lane", () => {
  const ci = workflow("nuncio-crew-ci.yml");
  const integStart = ci.indexOf("desktop-e2e-integration:");
  assert.ok(integStart > 0, "desktop-e2e-integration job must exist");
  const gateStart = ci.indexOf("\n  gate:", integStart);
  const integ = ci.slice(
    integStart,
    gateStart > integStart ? gateStart : undefined,
  );

  assert.match(integ, /name:\s*Desktop E2E Integration/);
  assert.match(integ, /continue-on-error:\s*true/);
  assert.match(integ, /shard:\s*\[1,\s*2\]/);
  assert.match(integ, /docker compose up -d postgres redis minio minio-init/);
  assert.match(integ, /cargo build --profile ci -p buzz-relay/);
  assert.match(integ, /BUZZ_RECONCILE_CHANNELS=true/);
  assert.match(integ, /setup-desktop-test-data\.sh/);
  assert.match(integ, /pnpm -C desktop build:e2e/);
  assert.match(
    integ,
    /playwright test --project=integration --shard=\$\{\{ matrix\.shard \}\}\/2/,
  );
  assert.match(integ, /needs\.changes\.outputs\.desktop == 'true'/);
  // Advisory (D-047): must not be registered in the merge gate.
  assert.doesNotMatch(ci, /needs\.desktop-e2e-integration\.result/);
  const gateHelper = readFileSync(gateHelperPath, "utf8");
  assert.doesNotMatch(gateHelper, /desktop-e2e-integration/);
  // Gate needs list must stay free of the integration job id.
  const gateNeeds = ci.match(
    /name:\s*NuncioCrew Gate[\s\S]*?needs:\s*\[([^\]]+)\]/,
  );
  assert.ok(gateNeeds, "gate needs list must be present");
  assert.doesNotMatch(gateNeeds[1], /desktop-e2e-integration/);
});

test("upstream compatibility is explicit and manual", () => {
  const upstream = workflow("nuncio-crew-upstream-sync.yml");
  const trigger = upstream.slice(
    upstream.indexOf("on:"),
    upstream.indexOf("\njobs:"),
  );

  assert.match(upstream, /^name:\s*NuncioCrew Upstream Sync$/m);
  assert.match(trigger, /^on:\s*\n\s+workflow_dispatch:/);
  assert.doesNotMatch(trigger, /^\s+(push|pull_request|schedule):/m);
  assert.match(upstream, /just fmt-check/);
  assert.match(upstream, /just clippy/);
  assert.match(upstream, /just test-unit/);
  assert.match(upstream, /just desktop-tauri-fmt-check/);
  assert.match(upstream, /just desktop-tauri-clippy/);
  assert.match(upstream, /just desktop-tauri-test/);
  assert.match(upstream, /cargo-deny check/);
});

test("merge gate accepts deliberately skipped conditional work", async () => {
  const { assertNuncioCrewCiResults } = await import(
    pathToFileURL(gateHelperPath).href
  );

  assert.doesNotThrow(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "success",
      desktop: "true",
      "desktop-rust-changed": "false",
      relay: "false",
      acp: "false",
      "desktop-fast": "success",
      "desktop-rust": "skipped",
      "macos-arm": "success",
      "project-relay": "skipped",
      "buzz-acp": "skipped",
    }),
  );
});

test("merge gate rejects a skipped Desktop Rust job when rust paths changed", async () => {
  const { assertNuncioCrewCiResults } = await import(
    pathToFileURL(gateHelperPath).href
  );

  // Skip acceptance lives in "deliberately skipped conditional work" above.
  // This PR always touches the workflow, so CI cannot prove the false branch
  // by running — the true+skipped reject is the complementary lock.
  assert.throws(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "success",
      desktop: "true",
      "desktop-rust-changed": "true",
      relay: "false",
      acp: "false",
      "desktop-fast": "success",
      "desktop-rust": "skipped",
      "macos-arm": "success",
      "project-relay": "skipped",
      "buzz-acp": "skipped",
    }),
  );
});

test("merge gate rejects a skipped buzz-acp job when acp paths changed", async () => {
  const { assertNuncioCrewCiResults } = await import(
    pathToFileURL(gateHelperPath).href
  );

  assert.throws(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "success",
      desktop: "true",
      "desktop-rust-changed": "true",
      relay: "false",
      acp: "true",
      "desktop-fast": "success",
      "desktop-rust": "success",
      "macos-arm": "success",
      "project-relay": "skipped",
      "buzz-acp": "skipped",
    }),
  );
});

test("merge gate rejects failed, cancelled, or missing dependencies", async () => {
  const { assertNuncioCrewCiResults } = await import(
    pathToFileURL(gateHelperPath).href
  );

  for (const result of ["failure", "cancelled", undefined]) {
    assert.throws(() =>
      assertNuncioCrewCiResults({
        "ci-policy": "success",
        desktop: "true",
        "desktop-rust-changed": "true",
        relay: "true",
        acp: "true",
        "desktop-fast": "success",
        "desktop-rust": "success",
        "macos-arm": result,
        "project-relay": "success",
        "buzz-acp": "success",
      }),
    );
  }
  assert.throws(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "skipped",
      desktop: "false",
      "desktop-rust-changed": "false",
      relay: "false",
      acp: "false",
      "desktop-fast": "skipped",
      "desktop-rust": "skipped",
      "macos-arm": "skipped",
      "project-relay": "skipped",
      "buzz-acp": "skipped",
    }),
  );
});

test("merge gate rejects a skipped relevant job or a run for irrelevant paths", async () => {
  const { assertNuncioCrewCiResults } = await import(
    pathToFileURL(gateHelperPath).href
  );

  assert.throws(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "success",
      desktop: "true",
      "desktop-rust-changed": "false",
      relay: "false",
      acp: "false",
      "desktop-fast": "success",
      "desktop-rust": "skipped",
      "macos-arm": "skipped",
      "project-relay": "skipped",
      "buzz-acp": "skipped",
    }),
  );
  assert.throws(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "success",
      desktop: "false",
      "desktop-rust-changed": "false",
      relay: "false",
      acp: "false",
      "desktop-fast": "success",
      "desktop-rust": "skipped",
      "macos-arm": "skipped",
      "project-relay": "skipped",
      "buzz-acp": "skipped",
    }),
  );
});
