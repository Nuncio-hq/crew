import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const VERSION = /^(\d+)\.(\d+)\.(\d+)(?:-dev(?:\.(\d+))?)?$/;

function parseVersion(value) {
  const match = VERSION.exec(value);
  if (!match) {
    throw new Error(`Unsupported NuncioCrew version "${value}"`);
  }
  return {
    core: match.slice(1, 4).map(Number),
    stable: match[4] === undefined && !value.includes("-dev"),
    devIteration: Number(match[4] ?? 0),
  };
}

export function compareNuncioCrewVersions(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);

  for (let index = 0; index < a.core.length; index += 1) {
    if (a.core[index] !== b.core[index]) {
      return a.core[index] < b.core[index] ? -1 : 1;
    }
  }
  if (a.stable !== b.stable) return a.stable ? 1 : -1;
  return Math.sign(a.devIteration - b.devIteration);
}

export function assertChannelAdvance(candidate, current) {
  if (compareNuncioCrewVersions(candidate, current) <= 0) {
    throw new Error(
      `Refusing updater rollback: ${candidate} does not follow ${current}`,
    );
  }
}

function main() {
  const [candidate, manifestPath] = process.argv.slice(2);
  if (!candidate || !manifestPath) {
    throw new Error(
      "Usage: assert-nuncio-crew-channel-advance.mjs <version> <latest.json>",
    );
  }
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (typeof manifest.version !== "string") {
    throw new Error("Current updater manifest has no version");
  }
  assertChannelAdvance(candidate, manifest.version);
  console.log(`Updater channel advances ${manifest.version} -> ${candidate}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
