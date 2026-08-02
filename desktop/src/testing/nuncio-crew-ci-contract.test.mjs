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
  for (const job of ["changes", "desktop-fast", "macos-arm", "project-relay"]) {
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

test("desktop smoke e2e runs on PRs as an advisory signal until flakes are triaged", () => {
  const ci = workflow("nuncio-crew-ci.yml");
  const smokeStart = ci.indexOf("desktop-smoke-e2e:");
  assert.ok(smokeStart > 0, "desktop-smoke-e2e job must exist");
  const gateStart = ci.indexOf("\n  gate:", smokeStart);
  const smoke = ci.slice(
    smokeStart,
    gateStart > smokeStart ? gateStart : undefined,
  );

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
      relay: "false",
      "desktop-fast": "success",
      "macos-arm": "success",
      "project-relay": "skipped",
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
        relay: "true",
        "desktop-fast": "success",
        "macos-arm": result,
        "project-relay": "success",
      }),
    );
  }
  assert.throws(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "skipped",
      desktop: "false",
      relay: "false",
      "desktop-fast": "skipped",
      "macos-arm": "skipped",
      "project-relay": "skipped",
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
      relay: "false",
      "desktop-fast": "success",
      "macos-arm": "skipped",
      "project-relay": "skipped",
    }),
  );
  assert.throws(() =>
    assertNuncioCrewCiResults({
      "ci-policy": "success",
      desktop: "false",
      relay: "false",
      "desktop-fast": "success",
      "macos-arm": "skipped",
      "project-relay": "skipped",
    }),
  );
});
