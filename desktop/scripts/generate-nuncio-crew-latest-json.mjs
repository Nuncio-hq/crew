import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { classifyNuncioCrewRelease } from "./nuncio-crew-release-channel.mjs";

export function buildNuncioCrewUpdateManifest({
  tag,
  signature,
  archiveUrl,
  publishedAt = new Date(),
}) {
  const release = classifyNuncioCrewRelease(tag);
  if (!signature.trim()) {
    throw new Error("Updater signature is empty");
  }
  return {
    version: release.version,
    notes: `NuncioCrew ${tag}`,
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
  const [tag, signaturePath, archiveUrl] = process.argv.slice(2);
  if (!tag || !signaturePath || !archiveUrl) {
    throw new Error(
      "Usage: generate-nuncio-crew-latest-json.mjs <tag> <signature-file> <archive-url>",
    );
  }
  const manifest = buildNuncioCrewUpdateManifest({
    tag,
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
