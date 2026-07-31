import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { classifyNuncioCrewRelease } from "./nuncio-crew-release-channel.mjs";

export function buildNuncioCrewUpdateManifest({
  versionTag,
  signature,
  archiveUrl,
  publishedAt = new Date(),
}) {
  const release = classifyNuncioCrewRelease(versionTag);
  if (!signature.trim()) {
    throw new Error("Updater signature is empty");
  }
  const archive = new URL(archiveUrl);
  const releasePath = `/Nuncio-hq/crew/releases/download/${release.releaseTag}/`;
  if (
    archive.origin !== "https://github.com" ||
    !archive.pathname.startsWith(releasePath)
  ) {
    throw new Error(
      `Updater archive must belong to immutable release ${release.releaseTag}`,
    );
  }
  return {
    version: release.version,
    notes: `NuncioCrew ${versionTag}`,
    pub_date: publishedAt.toISOString(),
    platforms: {
      "darwin-aarch64": {
        signature: signature.trim(),
        url: archiveUrl,
      },
    },
  };
}

function runCli() {
  const [versionTag, signaturePath, archiveUrl] = process.argv.slice(2);
  if (!versionTag || !signaturePath || !archiveUrl) {
    throw new Error(
      "Usage: generate-nuncio-crew-latest-json.mjs <version> <signature-file> <archive-url>",
    );
  }
  const manifest = buildNuncioCrewUpdateManifest({
    versionTag,
    signature: readFileSync(signaturePath, "utf8"),
    archiveUrl,
  });
  console.log(JSON.stringify(manifest, null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
