import { pathToFileURL } from "node:url";

// Job result keys and relevance keys share one flat object — they must never
// collide. The path-filter output is `desktop-rust`; the payload renames it to
// `desktop-rust-changed` so it cannot overwrite the `desktop-rust` job result.
const JOB_RELEVANCE = {
  "desktop-fast": "desktop",
  "macos-arm": "desktop",
  "desktop-rust": "desktop-rust-changed",
  "project-relay": "relay",
  postgres: "relay",
  "buzz-acp": "acp",
};

export function assertNuncioCrewCiResults(results) {
  if (results["ci-policy"] !== "success") {
    throw new Error(
      `CI Policy must succeed, got ${results["ci-policy"] ?? "missing"}.`,
    );
  }

  for (const [job, relevanceKey] of Object.entries(JOB_RELEVANCE)) {
    const relevance = results[relevanceKey];
    if (relevance !== "true" && relevance !== "false") {
      throw new Error(
        `${relevanceKey} relevance must be true or false, got ${
          relevance ?? "missing"
        }.`,
      );
    }

    const result = results[job];
    const expected = relevance === "true" ? "success" : "skipped";
    if (result !== expected) {
      throw new Error(
        `${job} must be ${expected} when ${relevanceKey}=${relevance}, got ${
          result ?? "missing"
        }.`,
      );
    }
  }
}

function main() {
  const raw = process.argv[2];
  if (!raw) {
    throw new Error("Expected one JSON object containing CI job results.");
  }
  const results = JSON.parse(raw);
  assertNuncioCrewCiResults(results);
  console.log("All required NuncioCrew CI results are acceptable.");
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
