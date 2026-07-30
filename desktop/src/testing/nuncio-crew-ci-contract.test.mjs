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
