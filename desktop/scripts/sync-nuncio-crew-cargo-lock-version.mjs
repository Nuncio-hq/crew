import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

export function replaceCargoLockPackageVersion(lockfile, packageName, version) {
  if (!packageName || !SEMVER.test(version)) {
    throw new Error("Expected a package name and a valid semver version");
  }

  let matches = 0;
  const updated = lockfile
    .split(/(?=^\[\[package\]\]$)/m)
    .map((block) => {
      if (!block.includes(`name = "${packageName}"`)) return block;

      matches += 1;
      const versionMatches = block.match(/^version = ".*"$/gm) ?? [];
      if (versionMatches.length !== 1) {
        throw new Error(
          `Expected one version for Cargo package ${packageName}`,
        );
      }
      return block.replace(/^version = ".*"$/m, `version = "${version}"`);
    })
    .join("");

  if (matches !== 1) {
    throw new Error(
      `Expected one Cargo.lock package named ${packageName}, found ${matches}`,
    );
  }
  return updated;
}

function main() {
  const packageName = process.argv[2];
  const version = process.argv[3];
  const lockfilePath = resolve(process.cwd(), "src-tauri/Cargo.lock");
  const lockfile = readFileSync(lockfilePath, "utf8");
  const updated = replaceCargoLockPackageVersion(
    lockfile,
    packageName,
    version,
  );

  writeFileSync(lockfilePath, updated);
  console.log(`Set Cargo.lock package ${packageName} to ${version}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
